//! Windowed-sinc resampler to 24 kHz stereo.
//!
//! The game's music is 24 kHz stereo EA-XA. Anything a user adds arrives at
//! 44.1 or 48 kHz, so every song goes through here once, before the encoder.
//! A 32-tap-per-side Hann-windowed sinc, normalized per output sample so the
//! DC gain is exactly one and the edges do not fade. Pure Rust, no crate.

use crate::soundtrack::SAMPLE_RATE;

/// Taps on each side of the center sample.
const HALF_TAPS: usize = 32;

/// Fold any channel count down to a stereo pair: one channel is duplicated,
/// two pass through, more than two average the even-index channels into the
/// left and the odd-index channels into the right. That is the right thing
/// for L/R/C/LFE/Ls/Rs layouts (left-ish channels are even, right-ish odd).
fn to_stereo(channels: &[Vec<f32>]) -> (Vec<f32>, Vec<f32>) {
    match channels.len() {
        0 => (Vec::new(), Vec::new()),
        1 => (channels[0].clone(), channels[0].clone()),
        2 => (channels[0].clone(), channels[1].clone()),
        n => {
            let len = channels.iter().map(Vec::len).min().unwrap_or(0);
            let (mut left, mut right) = (vec![0f32; len], vec![0f32; len]);
            let (mut nl, mut nr) = (0f32, 0f32);
            for (k, ch) in channels.iter().enumerate() {
                let dst = if k % 2 == 0 {
                    nl += 1.0;
                    &mut left
                } else {
                    nr += 1.0;
                    &mut right
                };
                for (d, s) in dst.iter_mut().zip(ch) {
                    *d += *s;
                }
            }
            let _ = n;
            for v in &mut left {
                *v /= nl.max(1.0);
            }
            for v in &mut right {
                *v /= nr.max(1.0);
            }
            (left, right)
        }
    }
}

/// -1.0..1.0 to a 16-bit sample, rounding half away from zero.
fn to_i16(v: f32) -> i16 {
    (f64::from(v) * 32767.0).round().clamp(-32768.0, 32767.0) as i16
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// One channel from `rate` to 24 kHz.
fn resample_channel(input: &[f32], rate: u32) -> Vec<i16> {
    let out_rate = f64::from(SAMPLE_RATE);
    let in_rate = f64::from(rate);
    let ratio = in_rate / out_rate; // input samples per output sample
    let out_len = ((input.len() as f64) * out_rate / in_rate).round() as usize;
    // Cutoff relative to the input rate: half the lower of the two rates.
    let cutoff = (in_rate.min(out_rate) / 2.0) / in_rate;
    let mut out = Vec::with_capacity(out_len);
    let taps = HALF_TAPS as f64;
    for i in 0..out_len {
        let center = (i as f64) * ratio;
        let base = center.floor() as i64;
        let mut acc = 0f64;
        let mut norm = 0f64;
        for k in (base - HALF_TAPS as i64 + 1)..=(base + HALF_TAPS as i64) {
            let d = (k as f64) - center;
            if d.abs() >= taps {
                continue;
            }
            let window = 0.5 * (1.0 + (std::f64::consts::PI * d / taps).cos());
            let w = 2.0 * cutoff * sinc(2.0 * cutoff * d) * window;
            norm += w;
            if k >= 0 && (k as usize) < input.len() {
                acc += w * f64::from(input[k as usize]);
            }
        }
        let v = if norm.abs() > 1e-9 { acc / norm } else { 0.0 };
        out.push((v * 32767.0).round().clamp(-32768.0, 32767.0) as i16);
    }
    out
}

/// Any channel layout at any rate to 24 kHz stereo 16-bit.
pub fn to_24k(channels: &[Vec<f32>], rate: u32) -> (Vec<i16>, Vec<i16>) {
    let (left, right) = to_stereo(channels);
    if rate == SAMPLE_RATE || rate == 0 {
        return (left.iter().map(|&v| to_i16(v)).collect(), right.iter().map(|&v| to_i16(v)).collect());
    }
    (resample_channel(&left, rate), resample_channel(&right, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_only_converts_and_rounds_half_away_from_zero() {
        let (l, r) = to_24k(&[vec![0.0, 0.5, -0.5, 1.0, -1.0, 2.0]], 24_000);
        assert_eq!(l, vec![0, 16384, -16384, 32767, -32767, 32767]);
        assert_eq!(l, r, "mono duplicates");
        // 0.5 * 32767 = 16383.5 rounds away from zero
        assert_eq!(to_i16(0.5), 16384);
        assert_eq!(to_i16(-0.5), -16384);
    }

    #[test]
    fn stereo_passes_through_and_multichannel_folds() {
        let (l, r) = to_24k(&[vec![0.25], vec![-0.25]], 24_000);
        assert_eq!((l[0], r[0]), (8192, -8192));
        // four channels: L, R, C, LFE -> left = mean(L, C), right = mean(R, LFE)
        let (l, r) = to_24k(&[vec![1.0], vec![-1.0], vec![0.0], vec![0.0]], 24_000);
        assert_eq!((l[0], r[0]), (16384, -16384));
    }

    #[test]
    fn a_48k_signal_halves_in_length_and_keeps_a_low_tone() {
        let n = 48_000;
        let input: Vec<f32> = (0..n).map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin() * 0.5).collect();
        let (l, _) = to_24k(&[input], 48_000);
        assert_eq!(l.len(), 24_000);
        // the tone survives: compare against the same tone sampled at 24 kHz,
        // away from the edges
        let mut err = 0f64;
        for (i, &got) in l.iter().enumerate().take(23_000).skip(1000) {
            let want = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 24_000.0).sin() * 0.5 * 32767.0;
            err += (f64::from(got) - want).powi(2);
        }
        let rms = (err / 22_000.0).sqrt();
        assert!(rms < 60.0, "rms error {rms}");
    }

    #[test]
    fn dc_gain_is_unity_and_output_length_rounds() {
        let (l, _) = to_24k(&[vec![0.5; 44_100]], 44_100);
        assert_eq!(l.len(), 24_000);
        assert!(l[1000..23_000].iter().all(|&v| (v - 16384).abs() <= 1), "{:?}", &l[12_000..12_010]);
        let (l, _) = to_24k(&[vec![0.0; 3]], 48_000);
        assert_eq!(l.len(), 2);
        let (l, _) = to_24k(&[], 48_000);
        assert!(l.is_empty());
    }
}
