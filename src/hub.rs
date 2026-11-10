//! Prebuffer: a persistent RTSP connection to the camera and its last 30 s in memory (not
//! decoded): new windows start from the last GOP, a replay from further back. Audio (when
//! enabled) is only forwarded live, without buffering. Shared by the Windows player and the
//! Linux ffmpeg pipeline: subscribers are plain functions.

use crate::audio::AudioTrack;
use crate::history::History;
use crate::rtp::{AccessUnit, VideoTrack};
use crate::rtsp::Media;
use crate::util::Cancel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a subscriber gets: the tracks, video access units, audio frames, or the end.
pub enum Feed {
    Track(VideoTrack, Option<AudioTrack>),
    Au(Arc<AccessUnit>),
    Audio(Arc<Vec<u8>>),
    Failed(String),
}

/// A subscriber; `false` = it is gone (the window closed).
pub type Sink = Box<dyn Fn(Feed) -> bool + Send>;

pub struct Hub {
    inner: Arc<Inner>,
}

struct Inner {
    url: String,
    audio: bool,
    cancel: Cancel,
    state: Mutex<State>,
}

struct State {
    track: Option<(VideoTrack, Option<AudioTrack>)>,
    history: History,
    subs: Vec<Sink>,
}

/// How far back the prebuffer reaches (a replay starts 5 s before the event).
const KEEP: Duration = Duration::from_secs(30);
/// At most this much of it: a camera with long GOPs must still start from the prebuffer.
const MAX_BYTES: usize = 24 << 20;

impl Hub {
    pub fn start(url: String, camera: &str, audio: bool) -> Hub {
        let state = State { track: None, history: History::new(Some(KEEP), MAX_BYTES), subs: Vec::new() };
        let inner = Arc::new(Inner { url, audio, cancel: Cancel::new(), state: Mutex::new(state) });
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

    /// Attaches a subscriber: it gets the tracks and current GOP at once, then subsequent frames.
    /// `false` if the stream is currently down.
    pub fn subscribe(&self, sink: Sink) -> bool {
        let mut st = self.inner.state.lock().unwrap();
        let Some((video, audio)) = st.track.clone() else { return false };
        if st.history.is_empty() || !sink(Feed::Track(video, audio)) {
            return false;
        }
        for au in st.history.last_gop() {
            let _ = sink(Feed::Au(au.clone()));
        }
        st.subs.push(sink);
        true
    }

    /// The kept video from the key frame at or before `at`, with its track (a replay).
    pub fn replay_from(&self, at: Instant) -> Option<(VideoTrack, Vec<Arc<AccessUnit>>)> {
        let st = self.inner.state.lock().unwrap();
        let (video, _) = st.track.clone()?;
        let units = st.history.from(at);
        (!units.is_empty()).then_some((video, units))
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
                st.history.clear();
                st.subs.retain(|s| s(Feed::Track(video.clone(), audio.cloned())));
            },
            |media| {
                let mut st = inner.state.lock().unwrap();
                match media {
                    Media::Video(au) => {
                        let au = Arc::new(au);
                        st.history.push(au.clone());
                        st.subs.retain(|s| s(Feed::Au(au.clone())));
                    }
                    Media::Audio(frame) => {
                        if !st.subs.is_empty() {
                            let frame = Arc::new(frame);
                            st.subs.retain(|s| s(Feed::Audio(frame.clone())));
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
            st.history.clear();
            for s in st.subs.drain(..) {
                s(Feed::Failed(err.clone()));
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
