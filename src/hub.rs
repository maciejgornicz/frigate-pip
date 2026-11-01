//! Prebuffer: a persistent RTSP connection to the camera and the last GOP in memory (not decoded).
//! Audio (when enabled) is only forwarded live, without buffering.

use crate::audio::AudioTrack;
use crate::player::{Input, SRC_PREBUF};
use crate::rtp::{AccessUnit, VideoTrack};
use crate::rtsp::Media;
use crate::util::Cancel;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub struct Hub {
    inner: Arc<Inner>,
}

struct Inner {
    url: String,
    audio: bool,
    cancel: Cancel,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    track: Option<(VideoTrack, Option<AudioTrack>)>,
    gop: Vec<Arc<AccessUnit>>,
    bytes: usize,
    subs: Vec<Sender<Input>>,
}

const MAX_GOP_BYTES: usize = 24 << 20;

impl Hub {
    pub fn start(url: String, camera: &str, audio: bool) -> Hub {
        let inner = Arc::new(Inner { url, audio, cancel: Cancel::new(), state: Mutex::new(State::default()) });
        let worker = inner.clone();
        let name = camera.to_string();
        let _ = std::thread::Builder::new().name(format!("prebuf-{camera}")).spawn(move || run(worker, name));
        Hub { inner }
    }

    pub fn url(&self) -> &str {
        &self.inner.url
    }

    pub fn audio(&self) -> bool {
        self.inner.audio
    }

    /// Attaches a receiver: it gets the tracks and current GOP at once, then subsequent frames.
    /// `false` if the stream is currently down.
    pub fn subscribe(&self, tx: &Sender<Input>) -> bool {
        let mut st = self.inner.state.lock().unwrap();
        let Some((video, audio)) = st.track.clone() else { return false };
        if st.gop.is_empty() || tx.send(Input::Track(SRC_PREBUF, video, audio)).is_err() {
            return false;
        }
        for au in &st.gop {
            let _ = tx.send(Input::Au(SRC_PREBUF, au.clone()));
        }
        st.subs.push(tx.clone());
        true
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.inner.cancel.cancel();
    }
}

fn run(inner: Arc<Inner>, camera: String) {
    let delays = [1000u64, 2000, 5000, 10000];
    let mut attempt = 0;
    let mut last_err = String::new();
    while !inner.cancel.is_set() {
        let started = std::time::Instant::now();
        let res = crate::rtsp::run(
            &inner.url,
            &inner.cancel,
            inner.audio,
            |video, audio| {
                let mut st = inner.state.lock().unwrap();
                st.track = Some((video.clone(), audio.cloned()));
                st.gop.clear();
                st.bytes = 0;
                st.subs.retain(|s| s.send(Input::Track(SRC_PREBUF, video.clone(), audio.cloned())).is_ok());
            },
            |media| {
                let mut st = inner.state.lock().unwrap();
                match media {
                    Media::Video(au) => {
                        let au = Arc::new(au);
                        if au.key {
                            st.gop.clear();
                            st.bytes = 0;
                        }
                        if !st.gop.is_empty() || au.key {
                            st.bytes += au.data.len();
                            st.gop.push(au.clone());
                            if st.bytes > MAX_GOP_BYTES {
                                st.gop.clear();
                                st.bytes = 0;
                            }
                        }
                        st.subs.retain(|s| s.send(Input::Au(SRC_PREBUF, au.clone())).is_ok());
                    }
                    Media::Audio(frame) => {
                        if !st.subs.is_empty() {
                            let frame = Arc::new(frame);
                            st.subs.retain(|s| s.send(Input::Audio(SRC_PREBUF, frame.clone())).is_ok());
                        }
                    }
                }
                true
            },
        );
        let err = match res {
            Ok(()) => "ended".to_string(),
            Err(e) => e,
        };
        {
            let mut st = inner.state.lock().unwrap();
            st.track = None;
            st.gop.clear();
            st.bytes = 0;
            for s in st.subs.drain(..) {
                let _ = s.send(Input::LiveFailed(SRC_PREBUF, err.clone()));
            }
        }
        if inner.cancel.is_set() {
            break;
        }
        if started.elapsed().as_secs() > 60 {
            attempt = 0;
        }
        if err != last_err {
            crate::log!("Prebuffer {camera}: {err} (retrying in background)");
            last_err = err;
        }
        inner.cancel.sleep(delays[attempt.min(delays.len() - 1)]);
        attempt += 1;
    }
}
