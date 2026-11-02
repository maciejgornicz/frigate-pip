//! Camera audio: audio track from SDP, AAC (RFC 3640) and G.711 depacketization,
//! AAC decoding via Media Foundation and playback via WASAPI.
//!
//! Everything except depacketization runs on a separate thread, so audio never
//! delays the appearance of the window or the video.

#[derive(Clone, Debug, PartialEq)]
pub enum AudioCodec {
    /// MPEG4-GENERIC (AAC-hbr / AAC-lbr): AudioSpecificConfig and AU header field lengths.
    Aac {
        config: Vec<u8>,
        size_len: u32,
        index_len: u32,
        index_delta_len: u32,
    },
    Pcmu,
    Pcma,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioTrack {
    pub codec: AudioCodec,
    pub payload_type: u8,
    pub control: String,
    pub rate: u32,
    pub channels: u16,
}

/// First audio track with a supported codec. Skips the backchannel
/// (to the camera speaker), marked in the SDP as `a=sendonly`.
pub fn parse_sdp_audio(sdp: &str) -> Option<AudioTrack> {
    struct Sec {
        pt: u8,
        name: String,
        rate: u32,
        channels: u16,
        fmtp: String,
        control: String,
        backchannel: bool,
    }
    let mut sections: Vec<Sec> = Vec::new();
    let mut in_audio = false;
    for line in sdp.lines().map(str::trim) {
        if let Some(m) = line.strip_prefix("m=") {
            in_audio = m.starts_with("audio");
            if in_audio {
                let pt = m.split_whitespace().nth(3).and_then(|p| p.parse().ok()).unwrap_or(0);
                sections.push(Sec { pt, name: String::new(), rate: 0, channels: 1, fmtp: String::new(), control: String::new(), backchannel: false });
            }
            continue;
        }
        let Some(s) = sections.last_mut().filter(|_| in_audio) else { continue };
        if let Some(r) = line.strip_prefix("a=rtpmap:") {
            let enc = r.split_whitespace().nth(1).unwrap_or("");
            let mut parts = enc.split('/');
            s.name = parts.next().unwrap_or("").to_ascii_uppercase();
            s.rate = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            s.channels = parts.next().and_then(|v| v.parse().ok()).unwrap_or(1);
        } else if let Some(f) = line.strip_prefix("a=fmtp:") {
            s.fmtp = f.split_once(' ').map(|(_, p)| p.to_string()).unwrap_or_default();
        } else if let Some(c) = line.strip_prefix("a=control:") {
            s.control = c.trim().to_string();
        } else if line == "a=sendonly" {
            // Server-side description: "sendonly" = the camera only receives (backchannel).
            s.backchannel = true;
        }
    }
    for s in sections.into_iter().filter(|s| !s.backchannel) {
        let fmtp = |key: &str| {
            s.fmtp.split(';').find_map(|kv| {
                let (k, v) = kv.trim().split_once('=')?;
                k.trim().eq_ignore_ascii_case(key).then(|| v.trim().to_string())
            })
        };
        let codec = match (s.name.as_str(), s.pt) {
            ("MPEG4-GENERIC", _) => {
                let config = hex(&fmtp("config")?)?;
                AudioCodec::Aac {
                    config,
                    size_len: fmtp("sizelength").and_then(|v| v.parse().ok()).unwrap_or(13),
                    index_len: fmtp("indexlength").and_then(|v| v.parse().ok()).unwrap_or(3),
                    index_delta_len: fmtp("indexdeltalength").and_then(|v| v.parse().ok()).unwrap_or(3),
                }
            }
            ("PCMU", _) | ("", 0) => AudioCodec::Pcmu,
            ("PCMA", _) | ("", 8) => AudioCodec::Pcma,
            _ => continue,
        };
        let (rate, channels) = match &codec {
            AudioCodec::Aac { config, .. } => asc_format(config).unwrap_or((s.rate, s.channels)),
            _ => (if s.rate > 0 { s.rate } else { 8000 }, s.channels.max(1)),
        };
        return Some(AudioTrack { codec, payload_type: s.pt, control: s.control, rate, channels });
    }
    None
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// Sample rate and channel count from AudioSpecificConfig.
pub fn asc_format(asc: &[u8]) -> Option<(u32, u16)> {
    const RATES: [u32; 13] = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];
    if asc.len() < 2 {
        return None;
    }
    let idx = ((asc[0] & 0x07) << 1 | asc[1] >> 7) as usize;
    let channels = ((asc[1] >> 3) & 0x0F) as u16;
    Some((*RATES.get(idx)?, channels.clamp(1, 8)))
}

/// Assembles AAC frames from RTP packets (RFC 3640, including frames split across packets).
pub struct AacDepacketizer {
    size_len: u32,
    index_len: u32,
    index_delta_len: u32,
    frag: Vec<u8>,
    frag_size: usize,
}

impl AacDepacketizer {
    pub fn new(size_len: u32, index_len: u32, index_delta_len: u32) -> Self {
        AacDepacketizer { size_len, index_len, index_delta_len, frag: Vec::new(), frag_size: 0 }
    }

    pub fn push(&mut self, p: &[u8], out: &mut Vec<Vec<u8>>) {
        if p.len() < 2 {
            return;
        }
        let header_bits = u16::from_be_bytes([p[0], p[1]]) as usize;
        let header_bytes = header_bits.div_ceil(8);
        let Some(headers) = p.get(2..2 + header_bytes) else { return };
        let mut data = &p[2 + header_bytes..];
        let mut bit = 0usize;
        let read = |bit: &mut usize, n: u32| -> Option<usize> {
            let mut v = 0usize;
            for _ in 0..n {
                let b = *headers.get(*bit / 8)?;
                v = v << 1 | ((b >> (7 - *bit % 8)) & 1) as usize;
                *bit += 1;
            }
            Some(v)
        };
        let mut sizes = Vec::new();
        while bit + (self.size_len as usize) <= header_bits {
            let Some(size) = read(&mut bit, self.size_len) else { break };
            let idx_len = if sizes.is_empty() { self.index_len } else { self.index_delta_len };
            if read(&mut bit, idx_len).is_none() {
                break;
            }
            sizes.push(size);
        }
        // Continuation of a frame split across several packets.
        if self.frag_size > 0 {
            if sizes.len() == 1 && sizes[0] == self.frag_size {
                self.frag.extend_from_slice(data);
                if self.frag.len() >= self.frag_size {
                    self.frag.truncate(self.frag_size);
                    push_raw(&std::mem::take(&mut self.frag), out);
                    self.frag_size = 0;
                }
                return;
            }
            self.frag.clear();
            self.frag_size = 0;
        }
        for size in sizes {
            if size <= data.len() {
                push_raw(&data[..size], out);
                data = &data[size..];
            } else {
                self.frag = data.to_vec();
                self.frag_size = size;
                return;
            }
        }
    }
}

/// Appends an AAC frame to `out`, stripping ADTS headers. Some cameras (e.g. some
/// Reolink models) put ADTS frames into RTP, and the decoder in raw AAC mode
/// accepts them without error but produces no sound.
fn push_raw(au: &[u8], out: &mut Vec<Vec<u8>>) {
    let is_adts = |b: &[u8]| b.len() >= 7 && b[0] == 0xFF && b[1] & 0xF6 == 0xF0;
    if !is_adts(au) {
        out.push(au.to_vec());
        return;
    }
    let mut rest = au;
    while is_adts(rest) {
        let header = if rest[1] & 0x01 != 0 { 7 } else { 9 };
        let len = ((rest[3] as usize & 0x03) << 11) | (rest[4] as usize) << 3 | (rest[5] as usize) >> 5;
        if len <= header || len > rest.len() {
            break;
        }
        out.push(rest[header..len].to_vec());
        rest = &rest[len..];
    }
}

pub fn ulaw_to_pcm(u: u8) -> i16 {
    let u = !u;
    let t = (((u & 0x0F) as i32) << 3) + 0x84;
    let t = t << ((u & 0x70) >> 4);
    (if u & 0x80 != 0 { 0x84 - t } else { t - 0x84 }) as i16
}

pub fn alaw_to_pcm(a: u8) -> i16 {
    let a = a ^ 0x55;
    let seg = (a & 0x70) >> 4;
    let mut t = ((a & 0x0F) as i32) << 4;
    t = match seg {
        0 => t + 8,
        1 => t + 0x108,
        _ => (t + 0x108) << (seg - 1),
    };
    (if a & 0x80 != 0 { t } else { -t }) as i16
}

#[cfg(windows)]
pub use playback::*;

/// AAC decoding (Media Foundation) and playback (WASAPI): Windows only.
#[cfg(windows)]
mod playback {
    use super::*;
    use crate::util::Cancel;
    use std::mem::ManuallyDrop;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, RecvTimeoutError, Sender};
    use std::sync::Arc;
    use std::time::Duration;

    use windows::core::Result;
    use windows::Win32::Foundation::E_FAIL;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

    enum Kind {
        Aac(IMFTransform),
        G711 { alaw: bool },
    }

    pub struct AudioDecoder {
        kind: Kind,
        pub rate: u32,
        pub channels: u16,
        /// Source format — preferred for the decoder output.
        native: (u32, u16),
        out_bytes: u32,
    }

    unsafe fn find_aac_decoder() -> Result<IMFTransform> {
        let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Audio, guidSubtype: MFAudioFormat_AAC };
        let flags = MFT_ENUM_FLAG(MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_LOCALMFT.0 | MFT_ENUM_FLAG_SORTANDFILTER.0);
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        MFTEnumEx(MFT_CATEGORY_AUDIO_DECODER, flags, Some(&input), None, &mut list, &mut count)?;
        let mut found = None;
        if !list.is_null() {
            for item in std::slice::from_raw_parts_mut(list, count as usize) {
                if let (None, Some(act)) = (&found, item.as_ref()) {
                    found = act.ActivateObject::<IMFTransform>().ok();
                }
                drop(item.take());
            }
            CoTaskMemFree(Some(list as *const _));
        }
        found.ok_or_else(|| E_FAIL.into())
    }

    impl AudioDecoder {
        pub fn new(t: &AudioTrack) -> Result<AudioDecoder> {
            match &t.codec {
                AudioCodec::Pcmu | AudioCodec::Pcma => Ok(AudioDecoder {
                    kind: Kind::G711 { alaw: t.codec == AudioCodec::Pcma },
                    rate: t.rate,
                    channels: t.channels,
                    native: (t.rate, t.channels),
                    out_bytes: 0,
                }),
                AudioCodec::Aac { config, .. } => unsafe {
                    if !crate::decoder::mf_startup() {
                        return Err(windows::Win32::Foundation::E_NOTIMPL.into());
                    }
                    let mft = find_aac_decoder()?;
                    let it = MFCreateMediaType()?;
                    it.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                    it.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
                    it.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, t.rate)?;
                    it.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, t.channels as u32)?;
                    it.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                    it.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
                    // HEAACWAVEINFO without WAVEFORMATEX: payload 0 (raw AAC), profile 0xFE (unspecified), then ASC.
                    // Note: for ≤ 24 kHz the Windows decoder reports 2× the sample rate and stereo anyway
                    // (it assumes implicit HE-AAC) — so the output format is read from the output type.
                    let mut user = vec![0u8, 0, 0xFE, 0, 0, 0, 0, 0, 0, 0, 0, 0];
                    user.extend_from_slice(config);
                    it.SetBlob(&MF_MT_USER_DATA, &user)?;
                    mft.SetInputType(0, &it, 0)?;
                    let mut d = AudioDecoder { kind: Kind::Aac(mft), rate: t.rate, channels: t.channels, native: (t.rate, t.channels), out_bytes: 0 };
                    d.set_output()?;
                    if let Kind::Aac(mft) = &d.kind {
                        mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
                    }
                    Ok(d)
                },
            }
        }

        /// Selects 16-bit PCM as the AAC decoder output (at startup and after a stream change).
        /// Prefers the stream format; the Windows decoder often reports 2× the sample rate and stereo.
        unsafe fn set_output(&mut self) -> Result<()> {
            let Kind::Aac(mft) = &self.kind else { return Ok(()) };
            let mut i = 0;
            let mut first = None;
            let mut exact = None;
            while let Ok(ot) = mft.GetOutputAvailableType(0, i) {
                i += 1;
                if ot.GetGUID(&MF_MT_SUBTYPE).ok() != Some(MFAudioFormat_PCM) || ot.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE).ok() != Some(16) {
                    continue;
                }
                let fmt = (ot.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).ok(), ot.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).ok());
                if fmt == (Some(self.native.0), Some(self.native.1 as u32)) {
                    exact = Some(ot);
                    break;
                }
                first.get_or_insert(ot);
            }
            let ot = exact.or(first).ok_or_else(|| windows::core::Error::from(E_FAIL))?;
            mft.SetOutputType(0, &ot, 0)?;
            self.rate = ot.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(self.rate);
            self.channels = ot.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).map(|c| c as u16).unwrap_or(self.channels);
            self.out_bytes = mft.GetOutputStreamInfo(0).map(|i| i.cbSize).unwrap_or(0).max(16384);
            Ok(())
        }

        pub fn decode(&mut self, frame: &[u8], out: &mut Vec<i16>) -> Result<()> {
            match &self.kind {
                Kind::G711 { alaw } => {
                    out.extend(frame.iter().map(|&b| if *alaw { alaw_to_pcm(b) } else { ulaw_to_pcm(b) }));
                    Ok(())
                }
                Kind::Aac(mft) => unsafe {
                    let mft = mft.clone();
                    let buf = MFCreateMemoryBuffer(frame.len() as u32)?;
                    let mut p = std::ptr::null_mut();
                    buf.Lock(&mut p, None, None)?;
                    std::ptr::copy_nonoverlapping(frame.as_ptr(), p, frame.len());
                    buf.Unlock()?;
                    buf.SetCurrentLength(frame.len() as u32)?;
                    let sample = MFCreateSample()?;
                    sample.AddBuffer(&buf)?;
                    mft.ProcessInput(0, &sample, 0)?;
                    loop {
                        let s = MFCreateSample()?;
                        s.AddBuffer(&MFCreateMemoryBuffer(self.out_bytes)?)?;
                        let mut ob = [MFT_OUTPUT_DATA_BUFFER {
                            dwStreamID: 0,
                            pSample: ManuallyDrop::new(Some(s)),
                            dwStatus: 0,
                            pEvents: ManuallyDrop::new(None),
                        }];
                        let mut status = 0u32;
                        let r = mft.ProcessOutput(0, &mut ob, &mut status);
                        let s = ManuallyDrop::take(&mut ob[0].pSample);
                        drop(ManuallyDrop::take(&mut ob[0].pEvents));
                        match r {
                            Ok(()) => {
                                let Some(s) = s else { continue };
                                let b = s.ConvertToContiguousBuffer()?;
                                let mut p = std::ptr::null_mut();
                                let mut len = 0u32;
                                b.Lock(&mut p, None, Some(&mut len))?;
                                let pcm = std::slice::from_raw_parts(p as *const i16, len as usize / 2);
                                out.extend_from_slice(pcm);
                                b.Unlock()?;
                            }
                            Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                            Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => self.set_output()?,
                            Err(e) => return Err(e),
                        }
                    }
                },
            }
        }
    }

    // ---------------------------------------------------------------- WASAPI output

    /// How much audio to buffer before starting, and the maximum kept in the queue.
    const START_MS: u32 = 100;
    const MAX_MS: u32 = 250;

    #[derive(Default)]
    struct OutStats {
        written: u32,
        skipped: u32,
    }

    struct AudioOut {
        client: IAudioClient,
        render: IAudioRenderClient,
        buffer_frames: u32,
        rate: u32,
        channels: u16,
        started: bool,
        stats: OutStats,
    }

    impl AudioOut {
        fn new(rate: u32, channels: u16) -> Result<AudioOut> {
            unsafe {
                let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
                let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
                let block = channels * 2;
                let wf = WAVEFORMATEX {
                    wFormatTag: WAVE_FORMAT_PCM as u16,
                    nChannels: channels,
                    nSamplesPerSec: rate,
                    nAvgBytesPerSec: rate * block as u32,
                    nBlockAlign: block,
                    wBitsPerSample: 16,
                    cbSize: 0,
                };
                // 300 ms buffer; WASAPI converts the format to the device format itself.
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    3_000_000,
                    0,
                    &wf,
                    None,
                )?;
                let buffer_frames = client.GetBufferSize()?;
                let render: IAudioRenderClient = client.GetService()?;
                Ok(AudioOut { client, render, buffer_frames, rate, channels, started: false, stats: OutStats::default() })
            }
        }

        /// Appends samples; when too much audio is already queued, the frame is
        /// dropped so that latency does not grow.
        fn write(&mut self, pcm: &[i16]) -> Result<()> {
            unsafe {
                let frames = (pcm.len() / self.channels as usize) as u32;
                let padding = self.client.GetCurrentPadding()?;
                let max = (self.rate * MAX_MS / 1000).min(self.buffer_frames);
                if frames == 0 || padding + frames > max {
                    self.stats.skipped += frames;
                    return Ok(());
                }
                self.stats.written += frames;
                let p = self.render.GetBuffer(frames)?;
                std::ptr::copy_nonoverlapping(pcm.as_ptr(), p as *mut i16, (frames * self.channels as u32) as usize);
                self.render.ReleaseBuffer(frames, 0)?;
                if !self.started && padding + frames >= self.rate * START_MS / 1000 {
                    self.client.Start()?;
                    self.started = true;
                }
            }
            Ok(())
        }
    }

    impl Drop for AudioOut {
        fn drop(&mut self) {
            unsafe {
                let _ = self.client.Stop();
            }
        }
    }

    // ---------------------------------------------------------------- audio thread

    pub enum AudioMsg {
        /// Audio track of a source (0 = prebuffer, 1 = direct RTSP). The decoder and output
        /// are created at once, before the picture appears, so audio starts without delay.
        Track(usize, AudioTrack),
        /// Which source to listen to (the one whose picture is on screen).
        Select(usize),
        Frame(usize, Arc<Vec<u8>>),
    }

    /// Sound of one window, shared with its mute button.
    #[derive(Default)]
    pub struct Sound {
        /// Muted by the user: silence is played, so the sound comes back at once.
        pub muted: AtomicBool,
        /// Sound is playing (the mute button is shown only then).
        pub playing: AtomicBool,
    }

    fn open_out(camera: &str, d: &AudioDecoder) -> Option<AudioOut> {
        match AudioOut::new(d.rate, d.channels) {
            Ok(o) => Some(o),
            Err(e) => {
                crate::log!("{camera}: cannot open audio output: {e}");
                None
            }
        }
    }

    /// Starts the audio playback thread; it ends on cancellation or when the channel closes.
    pub fn spawn(camera: String, cancel: Cancel, sound: Arc<Sound>) -> Sender<AudioMsg> {
        let (tx, rx) = mpsc::channel::<AudioMsg>();
        let _ = std::thread::Builder::new().name(format!("audio-{camera}")).spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            {
                let mut decs: [Option<AudioDecoder>; 2] = [None, None];
                let mut out: Option<AudioOut> = None;
                let mut selected: Option<usize> = None;
                let mut pcm: Vec<i16> = Vec::with_capacity(4096);
                let mut failed = false;
                let mut first = true;
                let (mut recv, mut silent) = (0u32, 0u32);
                while !cancel.is_set() {
                    match rx.recv_timeout(Duration::from_millis(200)) {
                        Ok(AudioMsg::Track(s, t)) => {
                            decs[s] = match AudioDecoder::new(&t) {
                                Ok(d) => Some(d),
                                Err(e) => {
                                    crate::log!("{camera}: audio {:?} unavailable: {e}", t.codec);
                                    None
                                }
                            };
                            if out.is_none() {
                                out = decs[s].as_ref().and_then(|d| open_out(&camera, d));
                            }
                        }
                        Ok(AudioMsg::Select(s)) => {
                            if selected != Some(s) {
                                selected = Some(s);
                                if let Some(d) = &decs[s] {
                                    if out.as_ref().is_none_or(|o| (o.rate, o.channels) != (d.rate, d.channels)) {
                                        drop(out.take());
                                        out = open_out(&camera, d);
                                    }
                                }
                            }
                        }
                        Ok(AudioMsg::Frame(s, f)) => {
                            if selected != Some(s) {
                                continue;
                            }
                            let Some(d) = decs[s].as_mut() else { continue };
                            pcm.clear();
                            let decoded = d.decode(&f, &mut pcm);
                            recv += 1;
                            if pcm.is_empty() {
                                silent += 1;
                            }
                            // The decoder may change the format after the first frame.
                            if decoded.is_ok() && out.as_ref().is_none_or(|o| (o.rate, o.channels) != (d.rate, d.channels)) {
                                drop(out.take());
                                out = open_out(&camera, d);
                            }
                            let Some(o) = out.as_mut() else { continue };
                            if sound.muted.load(Ordering::Relaxed) {
                                pcm.fill(0);
                            }
                            match decoded.and_then(|_| o.write(&pcm)) {
                                Ok(()) if first => {
                                    first = false;
                                    sound.playing.store(true, Ordering::Relaxed);
                                    crate::log!("{camera}: audio {} Hz, {} ch", d.rate, d.channels);
                                }
                                Ok(()) => {}
                                Err(e) => {
                                    if !failed {
                                        crate::log!("{camera}: audio error: {e}");
                                        failed = true;
                                    }
                                }
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
                if let Some(o) = &out {
                    let secs = |frames: u32| frames as f32 / o.rate.max(1) as f32;
                    crate::log!(
                        "{camera}: audio - frames {recv} (without samples: {silent}), played {:.1} s, skipped {:.1} s",
                        secs(o.stats.written),
                        secs(o.stats.skipped)
                    );
                }
            }
            CoUninitialize();
        });
        tx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDP: &str = "v=0\r\nm=video 0 RTP/AVP 96\r\na=rtpmap:96 H264/90000\r\na=control:trackID=0\r\nm=audio 0 RTP/AVP 97\r\na=rtpmap:97 MPEG4-GENERIC/16000\r\na=fmtp:97 streamtype=5;profile-level-id=1;mode=AAC-hbr;sizelength=13;indexlength=3;indexdeltalength=3;config=1408\r\na=recvonly\r\na=control:trackID=1\r\n";

    #[test]
    fn parses_aac_track_from_go2rtc() {
        let t = parse_sdp_audio(SDP).unwrap();
        assert_eq!(t.payload_type, 97);
        assert_eq!(t.control, "trackID=1");
        assert_eq!((t.rate, t.channels), (16000, 1));
        assert_eq!(t.codec, AudioCodec::Aac { config: vec![0x14, 0x08], size_len: 13, index_len: 3, index_delta_len: 3 });
    }

    #[test]
    fn skips_backchannel_and_takes_g711() {
        let sdp = "m=video 0 RTP/AVP 96\r\nm=audio 0 RTP/AVP 0\r\na=rtpmap:0 PCMU/8000\r\na=sendonly\r\na=control:trackID=2\r\nm=audio 0 RTP/AVP 8\r\na=rtpmap:8 PCMA/8000\r\na=control:trackID=3\r\n";
        let t = parse_sdp_audio(sdp).unwrap();
        assert_eq!((t.codec, t.payload_type, t.rate, t.control.as_str()), (AudioCodec::Pcma, 8, 8000, "trackID=3"));
        assert!(parse_sdp_audio("m=video 0 RTP/AVP 96\r\na=rtpmap:96 H264/90000\r\n").is_none());
        assert!(parse_sdp_audio("m=audio 0 RTP/AVP 98\r\na=rtpmap:98 OPUS/48000/2\r\n").is_none());
    }

    #[test]
    fn audio_specific_config() {
        assert_eq!(asc_format(&[0x14, 0x08]), Some((16000, 1)));
        assert_eq!(asc_format(&[0x12, 0x10]), Some((44100, 2)));
        assert_eq!(asc_format(&[0x11, 0x90]), Some((48000, 2)));
        assert_eq!(asc_format(&[0x14]), None);
    }

    #[test]
    fn aac_hbr_single_and_multiple_aus() {
        let mut d = AacDepacketizer::new(13, 3, 3);
        let mut out = Vec::new();
        // 1 AU of size 3: 16-bit header = (3 << 3) | 0
        d.push(&[0x00, 0x10, 0x00, 0x18, 1, 2, 3], &mut out);
        assert_eq!(out, vec![vec![1, 2, 3]]);
        // 2 AUs: sizes 2 and 1
        out.clear();
        d.push(&[0x00, 0x20, 0x00, 0x10, 0x00, 0x08, 7, 8, 9], &mut out);
        assert_eq!(out, vec![vec![7, 8], vec![9]]);
    }

    #[test]
    fn aac_hbr_fragmented_au() {
        let mut d = AacDepacketizer::new(13, 3, 3);
        let mut out = Vec::new();
        // AU of size 5 in two packets (same header with the full size).
        d.push(&[0x00, 0x10, 0x00, 0x28, 1, 2, 3], &mut out);
        assert!(out.is_empty());
        d.push(&[0x00, 0x10, 0x00, 0x28, 4, 5], &mut out);
        assert_eq!(out, vec![vec![1, 2, 3, 4, 5]]);
    }

    #[test]
    fn strips_adts_header_sent_by_some_reolink_cameras() {
        // Header from a Reolink camera: AAC-LC, 16 kHz, mono, frame length 519 (7 + 512).
        let mut au = vec![0xff, 0xf1, 0x60, 0x40, 0x40, 0xff, 0xfc];
        au.extend((0..512).map(|i| i as u8));
        let mut payload = vec![0x00, 0x10];
        payload.extend_from_slice(&((au.len() as u16) << 3).to_be_bytes());
        payload.extend_from_slice(&au);
        let mut d = AacDepacketizer::new(13, 3, 3);
        let mut out = Vec::new();
        d.push(&payload, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 512);
        assert_eq!(out[0][..3], [0, 1, 2]);
        // Raw AAC (as sent by most cameras) is left unchanged.
        let mut out = Vec::new();
        d.push(&[0x00, 0x10, 0x00, 0x18, 0x01, 0x20, 0x35], &mut out);
        assert_eq!(out, vec![vec![0x01, 0x20, 0x35]]);
    }

    #[test]
    fn g711_reference_values() {
        assert_eq!(ulaw_to_pcm(0xFF), 0);
        assert_eq!(ulaw_to_pcm(0x7F), 0);
        assert_eq!(ulaw_to_pcm(0x00), -32124);
        assert_eq!(ulaw_to_pcm(0x80), 32124);
        assert_eq!(alaw_to_pcm(0xD5), 8);
        assert_eq!(alaw_to_pcm(0x55), -8);
        assert_eq!(alaw_to_pcm(0xAA), 32256);
        assert_eq!(alaw_to_pcm(0x2A), -32256);
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "requires Media Foundation"]
    fn creates_aac_decoder() {
        let t = parse_sdp_audio(SDP).unwrap();
        let mut d = AudioDecoder::new(&t).unwrap();
        println!("AAC decoder output: {} Hz, {} ch.", d.rate, d.channels);
        assert!(d.rate == 16000 || d.rate == 32000, "{}", d.rate);
        assert!(d.channels == 1 || d.channels == 2);
        // AAC-LC mono silence (an "empty" frame) — the decoder must not crash.
        let mut pcm = Vec::new();
        let _ = d.decode(&[0x01, 0x18, 0x20, 0x07], &mut pcm);
    }
}
