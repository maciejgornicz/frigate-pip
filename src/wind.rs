//! Wind filter for the camera audio: quiets wind blowing into the microphone and leaves
//! the rest at its full volume.
//!
//! Two things happen while the wind blows, because one of them is not enough.
//!
//! The sound goes through a **high-pass filter** whose cutoff follows the wind: 80 Hz in
//! calm air (nothing audible is lost), up to 500 Hz in a gust. That removes the rumble —
//! on a camera whose microphone the wind only shakes, this is the whole job.
//!
//! Air hitting the microphone head-on, though, drives it past what it can take, and a
//! microphone at its limit makes noise over the whole range — measured on a real camera, a
//! gust put as much energy between 300 and 1000 Hz as below 300 Hz, and a quarter of it
//! above 500 Hz. Filtering cannot bring back a wave that was already flattened, so while
//! the wind blows the filter also **turns the sound down**, by up to 18 dB, and lets go of
//! it as the gust passes. A voice is left alone in the calm, where it can be heard at all;
//! in a gust it is quieted along with the wind, where the wind would drown it anyway.
//!
//! Wind is recognised by the share of the sound's energy that lies below 70 Hz, where a
//! voice has next to nothing.

use std::f32::consts::PI;

/// Cutoff in calm air and in full wind.
const CALM_HZ: f32 = 80.0;
const WIND_HZ: f32 = 500.0;
/// How much the sound is turned down in full wind that is loud enough to hurt.
const DUCK_DB: f32 = 18.0;
/// Turned down only by what the high-pass could not deal with: nothing below `QUIET` is
/// touched, the full amount waits for `LOUD` (mean square, full scale = 1; about −40 and
/// −20 dB). Where the wind only rumbles, the filter alone quiets it and the volume is left
/// alone; the microphone the wind blows into straight on is the one that needs both.
const QUIET: f32 = 1e-4;
const LOUD: f32 = 1e-2;
/// Above this much of full scale the output is bent instead of cut off: a high-pass filter
/// overshoots on sound that already reaches the rails, and cutting that off screeches.
const SOFT: f32 = 0.7;
/// The detector looks at the energy below this frequency (steeply: 24 dB per octave)…
const DETECT_HZ: f32 = 70.0;
/// …and takes a share of the whole from `SHARE_NONE` (no wind) to `SHARE_FULL` (full wind).
/// A low male voice leaves a few percent of its energy there, wind most of its own, so
/// wind is still recognised while somebody speaks as loudly as it blows.
const SHARE_NONE: f32 = 0.05;
const SHARE_FULL: f32 = 0.25;
/// Quieter than this (mean square, full scale = 1; about −70 dB) there is nothing to judge.
const FLOOR: f32 = 1e-7;
/// Averaging time of the energies, and how fast the filter follows a gust and lets go of it.
const ENERGY_S: f32 = 0.02;
const ATTACK_S: f32 = 0.03;
const RELEASE_S: f32 = 0.8;
/// The cutoff is recomputed every this many samples.
const STEP: usize = 16;
/// Q of the two sections of a 4th-order Butterworth filter.
const Q: [f32; 2] = [0.541_196_1, 1.306_563];

/// State-variable filter (trapezoidal): stays stable while its cutoff moves.
#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

#[derive(Clone, Copy, Default)]
struct Coef {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl Coef {
    fn new(hz: f32, q: f32, rate: f32) -> Coef {
        let g = (PI * (hz / rate).min(0.45)).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        Coef { k, a1, a2: g * a1, a3: g * g * a1 }
    }
}

impl Svf {
    /// One sample: (low-pass, high-pass) outputs.
    fn tick(&mut self, c: &Coef, x: f32) -> (f32, f32) {
        let v3 = x - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, x - c.k * v1 - v2)
    }
}

pub struct WindFilter {
    rate: f32,
    channels: usize,
    /// Two filter sections per channel.
    stages: Vec<[Svf; 2]>,
    coef: [Coef; 2],
    detector: [Svf; 2],
    detector_coef: [Coef; 2],
    /// Smoothed energy below `DETECT_HZ` and of the whole sound.
    low: f32,
    all: f32,
    /// Smoothed energy of what comes out of the high-pass, before the volume.
    left_over: f32,
    /// How much wind there is, 0–1.
    wind: f32,
    /// What the sound is turned down by now, 0–1 (1 = not at all).
    gain: f32,
    /// Samples until the cutoff is recomputed.
    until_step: usize,
    energy_k: f32,
    attack_k: f32,
    release_k: f32,
}

/// Share of the way a value moves towards its target in one step of `seconds`-long smoothing.
fn smoothing(seconds: f32, steps_per_second: f32) -> f32 {
    1.0 - (-1.0 / (seconds * steps_per_second)).exp()
}

impl WindFilter {
    pub fn new(rate: u32, channels: u16) -> WindFilter {
        let rate = rate.max(1) as f32;
        let channels = channels.max(1) as usize;
        let mut f = WindFilter {
            rate,
            channels,
            stages: vec![[Svf::default(); 2]; channels],
            coef: [Coef::default(); 2],
            detector: [Svf::default(); 2],
            detector_coef: [Coef::new(DETECT_HZ, Q[0], rate), Coef::new(DETECT_HZ, Q[1], rate)],
            low: 0.0,
            all: 0.0,
            left_over: 0.0,
            wind: 0.0,
            gain: 1.0,
            until_step: 0,
            energy_k: smoothing(ENERGY_S, rate),
            attack_k: smoothing(ATTACK_S, rate / STEP as f32),
            release_k: smoothing(RELEASE_S, rate / STEP as f32),
        };
        f.set_cutoff();
        f
    }

    /// The format this filter was made for.
    pub fn format(&self) -> (u32, u16) {
        (self.rate as u32, self.channels as u16)
    }

    /// The cutoff the filter is at now, in Hz.
    pub fn cutoff(&self) -> f32 {
        CALM_HZ * (WIND_HZ / CALM_HZ).powf(self.wind)
    }

    /// What the sound is turned down by now, in decibels (0 = not at all).
    #[cfg(test)]
    pub fn turned_down_db(&self) -> f32 {
        -20.0 * self.gain.log10()
    }

    fn set_cutoff(&mut self) {
        let hz = self.cutoff();
        self.coef = [Coef::new(hz, Q[0], self.rate), Coef::new(hz, Q[1], self.rate)];
    }

    /// Follows the wind: called every `STEP` samples.
    fn step(&mut self) {
        let share = if self.all > FLOOR { self.low / self.all } else { 0.0 };
        let target = ((share - SHARE_NONE) / (SHARE_FULL - SHARE_NONE)).clamp(0.0, 1.0);
        let k = if target > self.wind { self.attack_k } else { self.release_k };
        self.wind += (target - self.wind) * k;
        // Both must hold to turn the sound down: it is wind, and the filter left it loud.
        let loud = ((self.left_over.max(QUIET) / QUIET).log10() / (LOUD / QUIET).log10()).clamp(0.0, 1.0);
        self.gain = 10f32.powf(-DUCK_DB * self.wind * loud / 20.0);
        self.set_cutoff();
    }

    /// Full scale without a hard edge: everything under `SOFT` passes untouched, the rest
    /// is bent towards the rail instead of being cut off at it.
    fn soft(x: f32) -> i16 {
        const FULL: f32 = 32767.0;
        let (m, sign) = (x.abs() / FULL, x.signum());
        let y = if m <= SOFT { m } else { SOFT + (1.0 - SOFT) * (1.0 - (-(m - SOFT) / (1.0 - SOFT)).exp()) };
        (sign * y * FULL) as i16
    }

    /// Filters interleaved 16-bit samples in place: the high-pass, then the volume the
    /// wind calls for.
    pub fn process(&mut self, pcm: &mut [i16]) {
        for frame in pcm.chunks_exact_mut(self.channels) {
            if self.until_step == 0 {
                self.until_step = STEP;
                self.step();
            }
            self.until_step -= 1;
            let mono = frame.iter().map(|s| *s as f32).sum::<f32>() / (self.channels as f32 * 32768.0);
            let (low, _) = self.detector[0].tick(&self.detector_coef[0], mono);
            let (low, _) = self.detector[1].tick(&self.detector_coef[1], low);
            self.low += (low * low - self.low) * self.energy_k;
            self.all += (mono * mono - self.all) * self.energy_k;
            let mut sum = 0.0;
            for (s, stage) in frame.iter_mut().zip(self.stages.iter_mut()) {
                let (_, x) = stage[0].tick(&self.coef[0], *s as f32);
                let (_, x) = stage[1].tick(&self.coef[1], x);
                sum += x;
                *s = Self::soft(x * self.gain);
            }
            let out = sum / (self.channels as f32 * 32768.0);
            self.left_over += (out * out - self.left_over) * self.energy_k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16000;

    fn tone(hz: f32, amplitude: f32, seconds: f32, rate: u32) -> Vec<f32> {
        (0..(seconds * rate as f32) as usize).map(|i| amplitude * (2.0 * PI * hz * i as f32 / rate as f32).sin()).collect()
    }

    /// Wind-like noise: white noise through a 12 dB per octave low-pass at 60 Hz.
    fn wind(amplitude: f32, seconds: f32, rate: u32) -> Vec<f32> {
        let mut seed = 12345u32;
        let k = 1.0 - (-2.0 * PI * 60.0 / rate as f32).exp();
        let (mut y, mut z) = (0.0f32, 0.0f32);
        let raw: Vec<f32> = (0..(seconds * rate as f32) as usize)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                y += ((seed >> 8) as f32 / 8388608.0 - 1.0 - y) * k;
                z += (y - z) * k;
                z
            })
            .collect();
        let peak = raw.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        raw.iter().map(|v| v / peak * amplitude).collect()
    }

    fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
        a.iter().zip(b).map(|(x, y)| x + y).collect()
    }

    fn pcm(signal: &[f32]) -> Vec<i16> {
        signal.iter().map(|v| (v * 32767.0) as i16).collect()
    }

    /// Amplitude of the `hz` component in the last `seconds` of `pcm` (full scale = 1).
    fn level(pcm: &[i16], hz: f32, seconds: f32, rate: u32) -> f32 {
        let n = (seconds * rate as f32) as usize;
        let tail = &pcm[pcm.len() - n..];
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, s) in tail.iter().enumerate() {
            let a = 2.0 * std::f64::consts::PI * hz as f64 * i as f64 / rate as f64;
            re += *s as f64 / 32768.0 * a.cos();
            im += *s as f64 / 32768.0 * a.sin();
        }
        (2.0 * (re * re + im * im).sqrt() / n as f64) as f32
    }

    fn rms(pcm: &[i16], seconds: f32, rate: u32) -> f32 {
        let n = (seconds * rate as f32) as usize;
        let tail = &pcm[pcm.len() - n..];
        (tail.iter().map(|s| (*s as f64 / 32768.0).powi(2)).sum::<f64>() / n as f64).sqrt() as f32
    }

    fn filtered(signal: &[f32], rate: u32) -> (Vec<i16>, WindFilter) {
        let mut f = WindFilter::new(rate, 1);
        let mut p = pcm(signal);
        // In uneven pieces, as the decoder delivers them.
        for piece in p.chunks_mut(333) {
            f.process(piece);
        }
        (p, f)
    }

    #[test]
    fn a_voice_alone_is_left_as_it_is() {
        // A low male voice: 110 Hz fundamental and its harmonics.
        let voice = mix(&mix(&tone(110.0, 0.2, 2.0, RATE), &tone(220.0, 0.2, 2.0, RATE)), &tone(880.0, 0.2, 2.0, RATE));
        let (out, f) = filtered(&voice, RATE);
        assert!(f.cutoff() < 90.0, "cutoff {}", f.cutoff());
        for hz in [110.0, 220.0, 880.0] {
            let l = level(&out, hz, 1.0, RATE);
            assert!((0.18..0.21).contains(&l), "{hz} Hz: {l}");
        }
    }

    #[test]
    fn wind_is_quieted() {
        let noise = wind(0.6, 3.0, RATE);
        let voice = tone(1000.0, 0.2, 3.0, RATE);
        let (out, f) = filtered(&mix(&noise, &voice), RATE);
        assert!(f.cutoff() > 400.0, "cutoff {}", f.cutoff());
        // Wind the filter alone cannot finish with (here: what is left above its cutoff)
        // is turned down as well.
        assert!(f.turned_down_db() > 15.0, "turned down {} dB", f.turned_down_db());
        // The gust came through the speakers at a fifth of the volume or less.
        let (before, after) = (rms(&pcm(&mix(&noise, &voice)), 1.0, RATE), rms(&out, 1.0, RATE));
        assert!(after < before / 5.0, "{before} -> {after}");
    }

    #[test]
    fn a_microphone_at_its_limit_does_not_screech() {
        // Wind that drives the microphone past full scale: the wave comes back flattened,
        // and the high-pass overshoots on it. Hard edges there are what screeches.
        let flat: Vec<f32> = wind(5.0, 3.0, RATE).iter().map(|v| v.clamp(-1.0, 1.0)).collect();
        let (out, _) = filtered(&flat, RATE);
        let at_the_rails = |p: &[i16]| p.iter().filter(|s| s.unsigned_abs() >= 32700).count();
        assert!(at_the_rails(&pcm(&flat)) > 1000, "the test signal should reach the rails");
        assert_eq!(at_the_rails(&out), 0, "the filter must not leave samples at the rails");
    }

    #[test]
    fn quiet_wind_is_only_filtered_not_turned_down() {
        // Far-off wind under a conversation is not what hurts: the rumble goes, the volume
        // stays, so a quiet scene is not made quieter still.
        let (out, f) = filtered(&wind(0.02, 3.0, RATE), RATE);
        assert!(f.cutoff() > 400.0, "cutoff {}", f.cutoff());
        assert!(f.turned_down_db() < 1.0, "turned down {} dB", f.turned_down_db());
        // The rumble itself is gone all the same.
        assert!(rms(&out, 1.0, RATE) < rms(&pcm(&wind(0.02, 3.0, RATE)), 1.0, RATE) / 3.0);
    }

    #[test]
    fn a_steady_rumble_is_removed() {
        let (out, _) = filtered(&tone(40.0, 0.5, 2.0, RATE), RATE);
        assert!(level(&out, 40.0, 1.0, RATE) < 0.001);
    }

    #[test]
    fn lets_go_when_the_wind_stops() {
        let mut signal = mix(&wind(0.6, 2.0, RATE), &tone(200.0, 0.2, 2.0, RATE));
        signal.extend(tone(200.0, 0.2, 4.0, RATE));
        let mut f = WindFilter::new(RATE, 1);
        let mut p = pcm(&signal);
        let (gust, calm) = p.split_at_mut(2 * RATE as usize);
        f.process(gust);
        assert!(f.cutoff() > 400.0, "in the gust {}", f.cutoff());
        // This gust is rumble alone, so the filter deals with it and the volume is left
        // alone; the low voice in it goes quiet with the wind.
        assert!(f.turned_down_db() < 1.0, "in the gust {} dB", f.turned_down_db());
        assert!(level(gust, 200.0, 0.5, RATE) < 0.02);
        f.process(calm);
        // …and comes back whole afterwards.
        assert!(f.cutoff() < 90.0, "afterwards {}", f.cutoff());
        assert!(f.turned_down_db() < 0.5, "afterwards {} dB", f.turned_down_db());
        let l = level(calm, 200.0, 1.0, RATE);
        assert!((0.18..0.21).contains(&l), "{l}");
    }

    #[test]
    fn telephone_quality_and_stereo() {
        // G.711: 8 kHz. The wind is recognised and quieted there too.
        let noisy = mix(&wind(0.6, 3.0, 8000), &tone(1000.0, 0.2, 3.0, 8000));
        let (out, f) = filtered(&noisy, 8000);
        assert!(f.cutoff() > 400.0);
        assert!(rms(&out, 1.0, 8000) < rms(&pcm(&noisy), 1.0, 8000) / 5.0);
        // A voice alone at 8 kHz keeps its volume.
        let (quiet, f) = filtered(&tone(1000.0, 0.2, 2.0, 8000), 8000);
        assert!(f.turned_down_db() < 0.5, "{} dB", f.turned_down_db());
        assert!((0.18..0.21).contains(&level(&quiet, 1000.0, 1.0, 8000)));
        // Stereo: each channel is filtered on its own, so nothing leaks between them.
        // (How much wind there is, is judged from both channels together.)
        let voice = tone(1000.0, 0.2, 2.0, 48000);
        let mut both: Vec<i16> = pcm(&voice).into_iter().map(|l| [l, 0]).flat_map(|p| p.into_iter()).collect();
        let mut f = WindFilter::new(48000, 2);
        assert_eq!(f.format(), (48000, 2));
        f.process(&mut both);
        let left: Vec<i16> = both.iter().step_by(2).copied().collect();
        let right: Vec<i16> = both.iter().skip(1).step_by(2).copied().collect();
        assert!((0.18..0.21).contains(&level(&left, 1000.0, 1.0, 48000)), "{}", level(&left, 1000.0, 1.0, 48000));
        assert!(right.iter().all(|s| *s == 0), "the silent channel must stay silent");
    }

    #[test]
    fn silence_stays_silence() {
        let mut f = WindFilter::new(RATE, 1);
        let mut p = vec![0i16; RATE as usize];
        f.process(&mut p);
        assert!(p.iter().all(|s| *s == 0));
        assert!(f.cutoff() < 81.0);
    }

    /// Runs a real recording through the filter, to hear and measure it on camera sound
    /// instead of test tones: `FRIGATEPIP_WIND_WAV=in.wav` writes `in.filtered.wav` beside
    /// it. 16-bit PCM only (`ffmpeg -i x -c:a pcm_s16le in.wav`). Prints what the filter did.
    #[test]
    #[ignore = "needs a recording: FRIGATEPIP_WIND_WAV=in.wav"]
    fn filters_a_recording() {
        let Ok(path) = std::env::var("FRIGATEPIP_WIND_WAV") else {
            println!("Set FRIGATEPIP_WIND_WAV to a 16-bit PCM wav file.");
            return;
        };
        let raw = std::fs::read(&path).expect("cannot read the file");
        // The format is in `fmt `, the samples in `data`; other chunks are skipped.
        let (mut rate, mut channels, mut data) = (0u32, 0u16, &raw[0..0]);
        let mut at = 12;
        while at + 8 <= raw.len() {
            let id = &raw[at..at + 4];
            let len = u32::from_le_bytes([raw[at + 4], raw[at + 5], raw[at + 6], raw[at + 7]]) as usize;
            let body = &raw[at + 8..(at + 8 + len).min(raw.len())];
            match id {
                b"fmt " if body.len() >= 16 => {
                    assert_eq!(u16::from_le_bytes([body[14], body[15]]), 16, "16-bit PCM only");
                    channels = u16::from_le_bytes([body[2], body[3]]);
                    rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                }
                b"data" => data = body,
                _ => {}
            }
            at += 8 + len + len % 2;
        }
        assert!(rate > 0 && channels > 0 && !data.is_empty(), "not a wav file");
        let mut pcm: Vec<i16> = data.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        let clipped = |p: &[i16]| p.iter().filter(|s| s.unsigned_abs() >= 32700).count();
        let rms = |p: &[i16]| (p.iter().map(|s| (*s as f64 / 32768.0).powi(2)).sum::<f64>() / p.len() as f64).sqrt();
        let (was, was_clipped) = (rms(&pcm), clipped(&pcm));
        let mut f = WindFilter::new(rate, channels);
        let mut cutoffs: Vec<f32> = Vec::new();
        // In pieces, as the decoder delivers them, noting where the cutoff went.
        for piece in pcm.chunks_mut(1024) {
            f.process(piece);
            cutoffs.push(f.cutoff());
        }
        let (lo, hi) = cutoffs.iter().fold((f32::MAX, 0.0f32), |(l, h), c| (l.min(*c), h.max(*c)));
        println!("{path}: {rate} Hz, {channels} ch, {:.1} s", pcm.len() as f32 / (rate * channels as u32) as f32);
        println!("  level  {:.1} dB -> {:.1} dB", 20.0 * was.log10(), 20.0 * rms(&pcm).log10());
        println!("  samples at the rails  {was_clipped} -> {}", clipped(&pcm));
        println!("  cutoff  {lo:.0}-{hi:.0} Hz, turned down by up to {:.1} dB", DUCK_DB);
        let mut out = raw[..data.as_ptr() as usize - raw.as_ptr() as usize].to_vec();
        out.extend(pcm.iter().flat_map(|s| s.to_le_bytes()));
        let path = path.replace(".wav", ".filtered.wav");
        std::fs::write(&path, out).expect("cannot write the result");
        println!("  written  {path}");
    }
}
