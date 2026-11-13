//! Wind filter for the camera audio: cuts the rumble of wind blowing into the microphone
//! and leaves voices at their full volume.
//!
//! Wind is mostly below 200–300 Hz and strongest below 100 Hz; what makes speech
//! understandable lies above 300 Hz. So the sound goes through a high-pass filter whose
//! cutoff follows the wind: 80 Hz in calm air (nothing audible is lost), up to 500 Hz in a
//! gust. Wind is recognised by the share of the sound's energy that lies below 70 Hz, where
//! a voice has next to nothing.

use std::f32::consts::PI;

/// Cutoff in calm air and in full wind.
const CALM_HZ: f32 = 80.0;
const WIND_HZ: f32 = 500.0;
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
    /// How much wind there is, 0–1.
    wind: f32,
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
            wind: 0.0,
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
        self.set_cutoff();
    }

    /// Filters interleaved 16-bit samples in place.
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
            for (s, stage) in frame.iter_mut().zip(self.stages.iter_mut()) {
                let (_, x) = stage[0].tick(&self.coef[0], *s as f32);
                let (_, x) = stage[1].tick(&self.coef[1], x);
                *s = x.clamp(-32768.0, 32767.0) as i16;
            }
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
    fn wind_is_cut_and_the_voice_in_it_stays() {
        let noise = wind(0.6, 3.0, RATE);
        let voice = tone(1000.0, 0.2, 3.0, RATE);
        let (out, f) = filtered(&mix(&noise, &voice), RATE);
        assert!(f.cutoff() > 400.0, "cutoff {}", f.cutoff());
        let l = level(&out, 1000.0, 1.0, RATE);
        assert!((0.18..0.21).contains(&l), "voice {l}");
        // What is left is the voice (0.2 / √2 = 0.141) with almost no wind in it.
        let (before, after) = (rms(&pcm(&noise), 1.0, RATE), rms(&out, 1.0, RATE));
        assert!(before > 0.1, "wind before {before}");
        assert!(after < 0.15, "after {after}");
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
        // In the gust the low part of the voice is cut with the wind…
        assert!(level(gust, 200.0, 0.5, RATE) < 0.02);
        f.process(calm);
        // …and comes back afterwards.
        assert!(f.cutoff() < 90.0, "afterwards {}", f.cutoff());
        let l = level(calm, 200.0, 1.0, RATE);
        assert!((0.18..0.21).contains(&l), "{l}");
    }

    #[test]
    fn telephone_quality_and_stereo() {
        // G.711: 8 kHz.
        let (out, f) = filtered(&mix(&wind(0.6, 3.0, 8000), &tone(1000.0, 0.2, 3.0, 8000)), 8000);
        assert!(f.cutoff() > 400.0);
        assert!((0.18..0.21).contains(&level(&out, 1000.0, 1.0, 8000)));
        // Stereo: both channels are filtered, each with its own state.
        let (noise, voice) = (wind(0.6, 3.0, 48000), tone(1000.0, 0.2, 3.0, 48000));
        let mut both: Vec<i16> = pcm(&mix(&noise, &voice)).into_iter().zip(pcm(&noise)).flat_map(|(l, r)| [l, r]).collect();
        let mut f = WindFilter::new(48000, 2);
        assert_eq!(f.format(), (48000, 2));
        f.process(&mut both);
        let left: Vec<i16> = both.iter().step_by(2).copied().collect();
        let right: Vec<i16> = both.iter().skip(1).step_by(2).copied().collect();
        assert!((0.18..0.21).contains(&level(&left, 1000.0, 1.0, 48000)));
        assert!(rms(&right, 1.0, 48000) < 0.01, "{}", rms(&right, 1.0, 48000));
    }

    #[test]
    fn silence_stays_silence() {
        let mut f = WindFilter::new(RATE, 1);
        let mut p = vec![0i16; RATE as usize];
        f.process(&mut p);
        assert!(p.iter().all(|s| *s == 0));
        assert!(f.cutoff() < 81.0);
    }
}
