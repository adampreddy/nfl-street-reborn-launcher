//! EA-XA v2 frame and block codec, stream builder.
//!
//! A byte-exact port of `encode_ea_xa.py` (nflstreet2_clean/tools). The
//! Python is the oracle: the segment test in `segments.rs` compares a whole
//! 40-segment blob against `extend_slot_40big.py` byte for byte, so every
//! rounding rule below is the Python's rule, not a "better" one.
//!
//! Layout facts (confirmed in game):
//! - Coefficients [0, 240, 460, 392] and [0, 0, -208, -220].
//! - ADPCM frame: 15 bytes, byte 0 = coef << 4 | (shift - 8), then 14 nibble
//!   bytes, 28 samples. PCM frame: 0xEE, two history samples, 28 samples, all
//!   big endian, 61 bytes.
//! - Block 0: 3 PCM + 55 ADPCM frames (1008 bytes, 1624 samples). Then a
//!   cycle of six 57-frame blocks (855 + 1 pad byte, 1596 samples) and one
//!   58-frame block (870 bytes, 1624 samples).
//! - Stream: SCHl {u32LE size = 8 + PT} PT; SCCl {12} u32BE block count;
//!   SCDl {u32LE size} u32BE samples, u32BE 0, u32BE bytes per channel, L
//!   frames, R frames; SCEl {8}.

use crate::platform::{Error, Result};
use crate::soundtrack::asf;

pub const COEF1: [i32; 4] = [0, 240, 460, 392];
pub const COEF2: [i32; 4] = [0, 0, -208, -220];

/// Silence: coef 0, shift 12, all zero nibbles.
const SILENCE_FRAME: [u8; 15] = [0x0C, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn invalid(msg: String) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

fn clamp16(v: i32) -> i32 {
    v.clamp(-32768, 32767)
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// Encode 28 samples into one EA-XA v2 ADPCM frame (15 bytes).
///
/// Tries the four coefficient sets and a window of shifts around the ideal
/// one, keeps the least squared error (strict less-than: the first best
/// wins, as in Python). Returns the frame and the decoder history after it.
pub fn encode_adpcm_frame(samples: &[i16; 28], hist1: i32, hist2: i32) -> ([u8; 15], i32, i32) {
    let mut best_error = i64::MAX;
    let mut best_frame: Option<(usize, u8, [u8; 28])> = None;
    let mut best_h1 = hist1;
    let mut best_h2 = hist2;

    for coef_idx in 0..4 {
        let c1 = i64::from(COEF1[coef_idx]);
        let c2 = i64::from(COEF2[coef_idx]);

        // Residual pass: history is the clamped input, not the decoded value.
        let mut h1_est = i64::from(hist1);
        let mut h2_est = i64::from(hist2);
        let mut max_residual: i64 = 0;
        for &s in samples.iter() {
            let s = i64::from(s);
            let target_val = s * 256 - c1 * h1_est - c2 * h2_est;
            if target_val.abs() > max_residual {
                max_residual = target_val.abs();
            }
            h2_est = h1_est;
            h1_est = s.clamp(-32768, 32767);
        }

        let center: i32 = if max_residual > 0 {
            let ideal = 28.0 - (max_residual as f64 / 7.0).max(1.0).log2();
            (ideal.round_ties_even() as i32).clamp(8, 23)
        } else {
            23
        };

        let lo = (center - 2).max(8);
        let hi = (center + 3).min(24);
        for shift in lo..hi {
            let shift_nibble = (shift - 8) as u8;
            let mut h1 = i64::from(hist1);
            let mut h2 = i64::from(hist2);
            let mut total_error: i64 = 0;
            let mut nibbles = [0u8; 28];

            for (k, &s) in samples.iter().enumerate() {
                let s = i64::from(s);
                let target_val = s * 256 - c1 * h1 - c2 * h2;
                let divisor = 1i64 << (28 - shift);
                let nib = (target_val as f64 / divisor as f64).round_ties_even() as i64;
                let nib = nib.clamp(-8, 7);
                let nib_u = (nib & 0xF) as u8;
                nibbles[k] = nib_u;

                let val = ((u32::from(nib_u) << 28) as i32) >> shift;
                let decoded = ((i64::from(val) + c1 * h1 + c2 * h2) >> 8).clamp(-32768, 32767);
                total_error += (decoded - s) * (decoded - s);
                h2 = h1;
                h1 = decoded;
            }

            if total_error < best_error {
                best_error = total_error;
                best_frame = Some((coef_idx, shift_nibble, nibbles));
                best_h1 = h1 as i32;
                best_h2 = h2 as i32;
            }
        }
    }

    let (coef_idx, shift_nibble, nibbles) = best_frame.expect("at least one shift is always tried");
    let mut frame = [0u8; 15];
    frame[0] = ((coef_idx as u8) << 4) | shift_nibble;
    for i in (0..28).step_by(2) {
        let hi = nibbles[i];
        let lo = if i + 1 < 28 { nibbles[i + 1] } else { 0 };
        frame[1 + i / 2] = (hi << 4) | lo;
    }
    (frame, best_h1, best_h2)
}

/// Encode 28 samples as a PCM frame (0xEE, 61 bytes).
pub fn encode_pcm_frame(samples: &[i16; 28], hist1: i32, hist2: i32) -> ([u8; 61], i32, i32) {
    let mut frame = [0u8; 61];
    frame[0] = 0xEE;
    frame[1..3].copy_from_slice(&(clamp16(hist1) as i16).to_be_bytes());
    frame[3..5].copy_from_slice(&(clamp16(hist2) as i16).to_be_bytes());
    for (i, &s) in samples.iter().enumerate() {
        frame[5 + i * 2..7 + i * 2].copy_from_slice(&s.to_be_bytes());
    }
    let new_h1 = i32::from(samples[27]);
    let new_h2 = i32::from(samples[26]);
    (frame, new_h1, new_h2)
}

fn chunk28(samples: &[i16], pos: usize) -> [i16; 28] {
    let mut out = [0i16; 28];
    let end = (pos + 28).min(samples.len());
    if pos < end {
        out[..end - pos].copy_from_slice(&samples[pos..end]);
    }
    out
}

/// Encode one mono channel into (block data, samples) pairs, following the
/// disc's block cycle exactly (see the module doc).
pub fn encode_channel_blocks(samples: &[i16]) -> Vec<(Vec<u8>, u32)> {
    let mut blocks = Vec::new();
    let total = samples.len();
    let mut pos = 0usize;
    let mut block_idx = 0usize;
    let mut hist1 = 0i32;
    let mut hist2 = 0i32;

    while pos < total {
        let mut block_data: Vec<u8> = Vec::with_capacity(1008);
        let mut block_samples: u32 = 0;

        if block_idx == 0 {
            for f in 0..58 {
                let chunk = chunk28(samples, pos);
                if f < 3 {
                    let (frame, h1, h2) = encode_pcm_frame(&chunk, hist1, hist2);
                    block_data.extend_from_slice(&frame);
                    hist1 = h1;
                    hist2 = h2;
                } else {
                    let (frame, h1, h2) = encode_adpcm_frame(&chunk, hist1, hist2);
                    block_data.extend_from_slice(&frame);
                    hist1 = h1;
                    hist2 = h2;
                }
                block_samples += 28;
                pos += 28;
                if pos >= total && f >= 2 {
                    break;
                }
            }
        } else {
            let cycle_pos = (block_idx - 1) % 7;
            let num_frames = if cycle_pos == 6 { 58 } else { 57 };
            for _ in 0..num_frames {
                if pos >= total {
                    break;
                }
                let chunk = chunk28(samples, pos);
                let (frame, h1, h2) = encode_adpcm_frame(&chunk, hist1, hist2);
                block_data.extend_from_slice(&frame);
                hist1 = h1;
                hist2 = h2;
                block_samples += 28;
                pos += 28;
            }
            if num_frames == 57 {
                block_data.push(0x00);
            }
        }

        if block_samples > 0 {
            blocks.push((block_data, block_samples));
        }
        block_idx += 1;
    }
    blocks
}

/// One stereo stream: SCHl + PT, SCCl, SCDl blocks of [L frames][R frames],
/// SCEl. With `target_size`, silent blocks pad the stream to exactly that
/// many bytes, the way the disc's own slots are filled.
pub fn encode_stereo_stream(left: &[i16], right: &[i16], pt: &[u8], target_size: Option<usize>) -> Vec<u8> {
    let mut l_blocks = encode_channel_blocks(left);
    let mut r_blocks = encode_channel_blocks(right);

    let pad_block = || {
        let mut d = Vec::with_capacity(856);
        for _ in 0..57 {
            d.extend_from_slice(&SILENCE_FRAME);
        }
        d.push(0);
        (d, 1596u32)
    };
    let max_blocks = l_blocks.len().max(r_blocks.len());
    while l_blocks.len() < max_blocks {
        l_blocks.push(pad_block());
    }
    while r_blocks.len() < max_blocks {
        r_blocks.push(pad_block());
    }

    let overhead = 8 + pt.len() + 12 + 8;

    let mut stereo_blocks: Vec<(Vec<u8>, Vec<u8>, u32)> = Vec::with_capacity(max_blocks + 8);
    for (l, r) in l_blocks.into_iter().zip(r_blocks) {
        let samples = l.1;
        stereo_blocks.push((l.0, r.0, samples));
    }

    if let Some(target) = target_size {
        let mut current_size = overhead;
        for (l, r, _) in &stereo_blocks {
            current_size += 8 + 12 + l.len() + r.len();
        }
        let mut block_idx = max_blocks;
        while current_size < target {
            let cycle_pos: i64 = if block_idx > 0 { ((block_idx - 1) % 7) as i64 } else { -1 };
            let (sil_data, sil_samples): (Vec<u8>, u32) = if block_idx == 0 || cycle_pos == 6 {
                let mut d = Vec::with_capacity(870);
                for _ in 0..58 {
                    d.extend_from_slice(&SILENCE_FRAME);
                }
                (d, 1624)
            } else {
                pad_block()
            };
            let block_cost = 8 + 12 + sil_data.len() * 2;
            if current_size + block_cost > target {
                let remaining = target as i64 - current_size as i64 - 8 - 12;
                if remaining >= 30 {
                    let per_ch = (remaining / 2) as usize;
                    let num_frames = per_ch / 15;
                    let leftover = per_ch - num_frames * 15;
                    let mut last = Vec::with_capacity(per_ch);
                    for _ in 0..num_frames {
                        last.extend_from_slice(&SILENCE_FRAME);
                    }
                    last.extend(std::iter::repeat_n(0u8, leftover));
                    // The Python also bumps current_size here; nothing reads
                    // it after the break, so the bump is left out.
                    stereo_blocks.push((last.clone(), last, (num_frames * 28) as u32));
                }
                break;
            } else {
                stereo_blocks.push((sil_data.clone(), sil_data, sil_samples));
                current_size += block_cost;
                block_idx += 1;
            }
        }
    }

    let mut stream = Vec::with_capacity(target_size.unwrap_or(overhead + stereo_blocks.len() * 1800));
    stream.extend_from_slice(b"SCHl");
    stream.extend_from_slice(&((8 + pt.len()) as u32).to_le_bytes());
    stream.extend_from_slice(pt);
    stream.extend_from_slice(b"SCCl");
    stream.extend_from_slice(&12u32.to_le_bytes());
    stream.extend_from_slice(&(stereo_blocks.len() as u32).to_be_bytes());
    for (l, r, samples) in &stereo_blocks {
        let bytes_per_ch = l.len();
        let chunk_size = 8 + 12 + l.len() + r.len();
        stream.extend_from_slice(b"SCDl");
        stream.extend_from_slice(&(chunk_size as u32).to_le_bytes());
        stream.extend_from_slice(&samples.to_be_bytes());
        stream.extend_from_slice(&0u32.to_be_bytes());
        stream.extend_from_slice(&(bytes_per_ch as u32).to_be_bytes());
        stream.extend_from_slice(l);
        stream.extend_from_slice(r);
    }
    stream.extend_from_slice(b"SCEl");
    stream.extend_from_slice(&8u32.to_le_bytes());
    stream
}

// ---------------------------------------------------------------------------
// Decoder
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub left: Vec<i16>,
    pub right: Vec<i16>,
    pub sample_rate: u32,
}

/// Decode one channel's frames until `want` samples are out. Frames are 15
/// bytes, or 61 when byte 0 is 0xEE (PCM). The trailing pad byte of a
/// 57-frame block is never reached because the sample count stops first.
fn decode_channel(data: &[u8], want: usize, out: &mut Vec<i16>) -> std::result::Result<(), String> {
    let mut h1: i32 = 0;
    let mut h2: i32 = 0;
    // History carries across frames within a channel's block data. Across
    // blocks the PCM frames at block 0 reseed it and ADPCM continues from the
    // previous decoded samples, which is what `out`'s tail holds.
    let n = out.len();
    if n >= 1 {
        h1 = i32::from(out[n - 1]);
    }
    if n >= 2 {
        h2 = i32::from(out[n - 2]);
    }
    let mut p = 0usize;
    let mut got = 0usize;
    while got < want {
        let b0 = *data.get(p).ok_or_else(|| format!("channel data ends after {got} of {want} samples"))?;
        if b0 == 0xEE {
            let f = data.get(p..p + 61).ok_or("truncated PCM frame")?;
            h1 = i32::from(i16::from_be_bytes([f[1], f[2]]));
            h2 = i32::from(i16::from_be_bytes([f[3], f[4]]));
            for i in 0..28 {
                let s = i16::from_be_bytes([f[5 + i * 2], f[6 + i * 2]]);
                out.push(s);
                h2 = h1;
                h1 = i32::from(s);
            }
            got += 28;
            p += 61;
        } else {
            let f = data.get(p..p + 15).ok_or("truncated ADPCM frame")?;
            let coef = (b0 >> 4) as usize;
            if coef > 3 {
                return Err(format!("frame byte 0x{b0:02X} names coefficient set {coef}"));
            }
            let shift = i32::from(b0 & 0xF) + 8;
            let c1 = COEF1[coef];
            let c2 = COEF2[coef];
            for &byte in &f[1..15] {
                for nib in [byte >> 4, byte & 0xF] {
                    let val = ((u32::from(nib) << 28) as i32) >> shift;
                    let sample = ((val + c1 * h1 + c2 * h2) >> 8).clamp(-32768, 32767);
                    out.push(sample as i16);
                    h2 = h1;
                    h1 = sample;
                }
            }
            got += 28;
            p += 15;
        }
    }
    Ok(())
}

/// Decode one SCHl..SCEl stream. Sample rate from PT 0x84 (default 24000),
/// channels from PT 0x82 (default 2); mono data is duplicated into both
/// channels so callers always get stereo.
pub fn decode_stream(stream: &[u8]) -> Result<Decoded> {
    if stream.get(0..4) != Some(b"SCHl") {
        return Err(invalid("stream does not start with SCHl".into()));
    }
    let pt = asf::pt_bytes(stream);
    let sample_rate = asf::pt_tag(pt, 0x84).map(|v| v as u32).unwrap_or(24_000);
    let channels = asf::pt_tag(pt, 0x82).unwrap_or(2);
    let expected = asf::pt_tag(pt, 0x85).map(|v| v as usize);
    let mut left: Vec<i16> = Vec::with_capacity(expected.unwrap_or(0));
    let mut right: Vec<i16> = Vec::with_capacity(expected.unwrap_or(0));

    let mut p = 0usize;
    let mut blocks = 0usize;
    loop {
        let tag = stream.get(p..p + 4).ok_or_else(|| invalid("stream ends without SCEl".into()))?;
        let size = asf::le32(stream, p + 4).ok_or_else(|| invalid("truncated chunk header".into()))? as usize;
        if size < 8 {
            return Err(invalid(format!("chunk at 0x{p:X} has size {size}")));
        }
        if tag == b"SCEl" {
            break;
        }
        if tag == b"SCDl" {
            let samples = asf::be32(stream, p + 8).ok_or_else(|| invalid("truncated SCDl".into()))? as usize;
            let bytes_per_ch = asf::be32(stream, p + 16).ok_or_else(|| invalid("truncated SCDl".into()))? as usize;
            let data = stream.get(p + 20..p + size).ok_or_else(|| invalid(format!("SCDl at 0x{p:X} runs past the stream")))?;
            let stereo = channels >= 2 && data.len() >= 2 * bytes_per_ch && bytes_per_ch > 0;
            if bytes_per_ch > data.len() {
                return Err(invalid(format!("SCDl at 0x{p:X}: bytes per channel {bytes_per_ch} > data {}", data.len())));
            }
            let l_data = &data[..bytes_per_ch];
            decode_channel(l_data, samples, &mut left).map_err(|e| invalid(format!("block {blocks} L: {e}")))?;
            if stereo {
                let r_data = &data[bytes_per_ch..2 * bytes_per_ch];
                decode_channel(r_data, samples, &mut right).map_err(|e| invalid(format!("block {blocks} R: {e}")))?;
            } else {
                let from = left.len() - samples.min(left.len());
                right.extend_from_slice(&left[from..]);
            }
            blocks += 1;
        }
        p += size;
    }
    Ok(Decoded { left, right, sample_rate })
}

/// Decode every stream in a concatenation (a slot blob, or a whole song), in
/// order, into one stereo signal.
pub fn decode_streams(concat: &[u8]) -> Result<Decoded> {
    let positions = asf::scan_schl(concat);
    if positions.is_empty() {
        return Err(invalid("no SCHl stream found".into()));
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut rate = 24_000;
    for (k, &start) in positions.iter().enumerate() {
        let len = asf::stream_len(concat, start).ok_or_else(|| invalid(format!("stream {k} has no SCEl")))?;
        let d = decode_stream(&concat[start..start + len])?;
        if k == 0 {
            rate = d.sample_rate;
        }
        left.extend_from_slice(&d.left);
        right.extend_from_slice(&d.right);
    }
    Ok(Decoded { left, right, sample_rate: rate })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PT header shaped like the disc's: preamble with 0xFD before the tag
    /// stream, then 0x80=3, 0x85 (3 bytes), 0x82=2, 0xA0=0x0A, 0x8C=4, 0xFF.
    const SYN_PT: &[u8] = &[
        0x50, 0x54, 0x06, 0x00, 0x06, 0x01, 0x65, 0x13, 0x01, 0x7F, 0xFD, 0x80, 0x01, 0x03, 0x85, 0x03,
        0x00, 0x00, 0x00, 0x82, 0x01, 0x02, 0xA0, 0x01, 0x0A, 0x8C, 0x01, 0x04, 0xFF, 0x00,
    ];

    #[test]
    fn silence_encodes_to_the_silence_frame() {
        let (f, h1, h2) = encode_adpcm_frame(&[0; 28], 0, 0);
        assert_eq!(f[0] & 0xF0, 0);
        assert!(f[1..].iter().all(|&b| b == 0));
        assert_eq!((h1, h2), (0, 0));
    }

    #[test]
    fn block_sizes_follow_the_cycle() {
        let s = vec![1000i16; 28 * 58 * 9];
        let b = encode_channel_blocks(&s);
        assert_eq!(b[0].0.len(), 1008);
        assert_eq!(b[0].1, 1624);
        assert_eq!(b[1].0.len(), 856);
        assert_eq!(b[1].1, 1596);
        assert_eq!(b[7].0.len(), 870);
        assert_eq!(b[7].1, 1624);
    }

    #[test]
    fn a_short_input_still_gets_three_pcm_frames() {
        let b = encode_channel_blocks(&[5i16; 10]);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].0.len(), 3 * 61);
        assert_eq!(b[0].1, 84);
    }

    #[test]
    fn a_stream_decodes_back_close_to_its_input() {
        let n = 24000usize;
        let left: Vec<i16> = (0..n).map(|i| ((i as f64 * 0.05).sin() * 12000.0) as i16).collect();
        let right: Vec<i16> = left.iter().map(|s| s / 2).collect();
        let st = encode_stereo_stream(&left, &right, SYN_PT, None);
        let d = decode_stream(&st).unwrap();
        assert!(d.left.len() >= n);
        assert_eq!(d.left.len(), d.right.len());
        assert_eq!(d.sample_rate, 24_000);
        let err: f64 = left.iter().zip(&d.left).map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2)).sum::<f64>() / n as f64;
        assert!(err.sqrt() < 200.0, "rms error {}", err.sqrt());
        let err_r: f64 = right.iter().zip(&d.right).map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2)).sum::<f64>() / n as f64;
        assert!(err_r.sqrt() < 200.0, "rms error R {}", err_r.sqrt());
        // the SCDl sample fields add up to what the decoder produced
        assert_eq!(asf::scdl_sample_sum(&st) as usize, d.left.len());
    }

    #[test]
    fn padded_stream_hits_the_target_exactly() {
        let st = encode_stereo_stream(&[100; 3000], &[100; 3000], SYN_PT, Some(0x4000));
        assert_eq!(st.len(), 0x4000);
        assert!(st.ends_with(b"SCEl\x08\x00\x00\x00"));
        let d = decode_stream(&st).unwrap();
        assert_eq!(d.left.len() as u64, asf::scdl_sample_sum(&st));
    }

    #[test]
    fn pt_helpers_read_and_patch() {
        assert_eq!(asf::pt_tag(SYN_PT, 0x82), Some(2));
        assert_eq!(asf::pt_tag(SYN_PT, 0x84), None);
        let p = asf::patch_pt_tag(SYN_PT, 0x85, 0x12345).unwrap();
        assert_eq!(p.len(), SYN_PT.len());
        assert_eq!(asf::pt_tag(&p, 0x85), Some(0x12345));
        assert!(asf::patch_pt_tag(SYN_PT, 0x85, 1 << 24).is_err());
        let st = encode_stereo_stream(&[1; 100], &[1; 100], &p, None);
        assert_eq!(asf::pt_bytes(&st), &p[..]);
        assert_eq!(asf::scan_schl(&st), vec![0]);
        assert_eq!(asf::stream_len(&st, 0), Some(st.len()));
    }

    #[test]
    fn decode_streams_joins_segments_in_order() {
        let a = encode_stereo_stream(&[1000; 500], &[1000; 500], SYN_PT, None);
        let b = encode_stereo_stream(&[-1000; 500], &[-1000; 500], SYN_PT, None);
        let mut cat = a.clone();
        cat.extend_from_slice(&b);
        let d = decode_streams(&cat).unwrap();
        let da = decode_stream(&a).unwrap();
        assert_eq!(d.left.len(), da.left.len() * 2);
        assert_eq!(&d.left[..da.left.len()], &da.left[..]);
    }
}
