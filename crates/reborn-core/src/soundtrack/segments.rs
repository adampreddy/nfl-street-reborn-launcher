//! The 40-segment slot encoder (extend_slot_40big.py recipe).
//!
//! A song becomes 40 uniform segments, each padded with silent blocks to
//! exactly the same byte size, each with its PT 0x85 sample count patched to
//! the sum the SCDl blocks actually carry. The engine credits SCDl samples
//! against PT 0x85 and frees the next, still virgin, descriptor when they
//! overshoot; EA authors sample-exact and so does this.
//!
//! The per-stream PT headers come from the target slot's 40 stock streams,
//! so a track is encoded for a slot.

use crate::platform::{Error, Result};
use crate::soundtrack::{asf, ea_xa};
use crate::soundtrack::{slot_first_stream, Refusal, Slot, SAMPLE_RATE, SEGMENTS_PER_SONG, SEGMENT_CEILING, STREAM_COUNT};

/// What the refusal message quotes as the ceiling. The real bound is the
/// per-segment byte ceiling, which depends on how well the audio compresses;
/// about 6:20 for typical music at 24 kHz.
pub const LIMIT_SECONDS_HINT: u64 = 380;

pub fn slot_limit_hint() -> u64 {
    LIMIT_SECONDS_HINT
}

fn invalid(msg: String) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

/// The 40 PT payloads of a slot's stock streams, in order.
pub fn stock_pt_headers(pristine_asf: &[u8], slot: Slot) -> Result<Vec<Vec<u8>>> {
    let pos = asf::scan_schl(pristine_asf);
    if pos.len() != STREAM_COUNT {
        return Err(invalid(format!("the ASF has {} streams, expected {STREAM_COUNT}", pos.len())));
    }
    let first = slot_first_stream(slot);
    let mut out = Vec::with_capacity(SEGMENTS_PER_SONG);
    for &p in &pos[first..first + SEGMENTS_PER_SONG] {
        let pt = asf::pt_bytes(&pristine_asf[p..]);
        if asf::pt_tag(pt, 0x85).is_none() {
            return Err(invalid(format!("stream at 0x{p:X} has no PT 0x85 sample count")));
        }
        out.push(pt.to_vec());
    }
    Ok(out)
}

/// Encode a 24 kHz stereo signal into a slot's 40-segment blob.
///
/// `on(k, 40)` is called before segment k is encoded. Refuses with
/// `Refusal::SongTooLong` when a segment would reach the ring-buffer ceiling.
pub fn encode_slot(left: &[i16], right: &[i16], pt_headers: &[Vec<u8>], on: &mut dyn FnMut(usize, usize)) -> Result<Vec<u8>> {
    if pt_headers.len() != SEGMENTS_PER_SONG {
        return Err(invalid(format!("{} PT headers, expected {SEGMENTS_PER_SONG}", pt_headers.len())));
    }
    if left.len() != right.len() {
        return Err(invalid(format!("left has {} samples, right has {}", left.len(), right.len())));
    }
    let total = left.len();
    if total == 0 {
        return Err(invalid("the song has no samples".into()));
    }
    let seg_samples = total.div_ceil(SEGMENTS_PER_SONG);

    // Trial-encode the first segment unpadded to size every segment.
    let trial_end = seg_samples.min(total);
    let trial = ea_xa::encode_stereo_stream(&left[..trial_end], &right[..trial_end], &pt_headers[0], None);
    let raw = trial.len();
    let target = raw.div_ceil(0x800) * 0x800 + 0x800;
    if target >= SEGMENT_CEILING {
        return Err(Refusal::SongTooLong {
            seconds: (total as u64) / u64::from(SAMPLE_RATE),
            limit_seconds: slot_limit_hint(),
        }
        .into());
    }

    let mut blob = Vec::with_capacity(target * SEGMENTS_PER_SONG);
    let silence = vec![0i16; seg_samples];
    for (k, pt_header) in pt_headers.iter().enumerate() {
        on(k, SEGMENTS_PER_SONG);
        let a = k * seg_samples;
        let b = ((k + 1) * seg_samples).min(total);
        let nsamp = b.saturating_sub(a);
        let pt = asf::patch_pt_tag(pt_header, 0x85, if nsamp > 0 { nsamp as u64 } else { 1 }).map_err(invalid)?;
        let mut st = if nsamp > 0 {
            ea_xa::encode_stereo_stream(&left[a..b], &right[a..b], &pt, Some(target))
        } else {
            ea_xa::encode_stereo_stream(&silence, &silence, &pt, Some(target))
        };
        if st.len() != target {
            return Err(invalid(format!("segment {k}: {} bytes, target {target}", st.len())));
        }
        // EA-exact sample accounting: PT 0x85 must equal the SCDl sum.
        let actual = asf::scdl_sample_sum(&st);
        if actual != nsamp as u64 {
            let schl_size = asf::le32(&st, 4).ok_or_else(|| invalid("truncated SCHl".into()))? as usize;
            let patched = asf::patch_pt_tag(&st[8..schl_size], 0x85, actual).map_err(invalid)?;
            st[8..schl_size].copy_from_slice(&patched);
        }
        blob.extend_from_slice(&st);
    }
    Ok(blob)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::{write_wav_24k, SYN_PT};
    use std::path::{Path, PathBuf};

    fn test_signal(seconds: usize) -> (Vec<i16>, Vec<i16>) {
        let n = seconds * SAMPLE_RATE as usize;
        let mut left = Vec::with_capacity(n);
        let mut right = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f64 / f64::from(SAMPLE_RATE);
            let env = 0.6 + 0.35 * (2.0 * std::f64::consts::PI * 0.1 * t).sin();
            let l = (2.0 * std::f64::consts::PI * 440.0 * t).sin() * env * 20000.0;
            let r = (2.0 * std::f64::consts::PI * 660.0 * t).sin() * env * 20000.0;
            left.push(l.round() as i16);
            right.push(r.round() as i16);
        }
        (left, right)
    }

    #[test]
    fn a_synthetic_slot_has_forty_uniform_sample_exact_segments() {
        let (left, right) = test_signal(3);
        let pts: Vec<Vec<u8>> = (0..SEGMENTS_PER_SONG).map(|_| SYN_PT.to_vec()).collect();
        let mut calls = 0;
        let blob = encode_slot(&left, &right, &pts, &mut |_, _| calls += 1).unwrap();
        assert_eq!(calls, 40);
        let pos = asf::scan_schl(&blob);
        assert_eq!(pos.len(), 40);
        let target = pos[1] - pos[0];
        assert_eq!(blob.len(), target * 40);
        assert_eq!(target % 0x800, 0);
        let mut total = 0u64;
        for &p in &pos {
            let st = &blob[p..];
            let pt85 = asf::pt_tag(asf::pt_bytes(st), 0x85).unwrap();
            let sum = asf::scdl_sample_sum(&st[..asf::stream_len(st, 0).unwrap()]);
            assert_eq!(pt85, sum, "PT 0x85 must equal the SCDl sum");
            total += sum;
        }
        assert!(total as usize >= left.len());
        let d = ea_xa::decode_streams(&blob).unwrap();
        assert_eq!(d.left.len() as u64, total);
    }

    #[test]
    fn a_song_past_the_ceiling_is_refused_with_its_length() {
        // Noise compresses worst; 7 minutes of it overflows the segment ceiling.
        let n = 420 * SAMPLE_RATE as usize;
        let mut x: u32 = 12345;
        let left: Vec<i16> = (0..n).map(|_| { x = x.wrapping_mul(1664525).wrapping_add(1013904223); (x >> 16) as i16 }).collect();
        let right = left.clone();
        let pts: Vec<Vec<u8>> = (0..SEGMENTS_PER_SONG).map(|_| SYN_PT.to_vec()).collect();
        let err = encode_slot(&left, &right, &pts, &mut |_, _| {}).unwrap_err();
        let m = err.to_string();
        assert!(m.contains("7:00") && m.contains("6:20"), "{m}");
    }

    /// Oracle 3: the Rust blob equals extend_slot_40big.py's, byte for byte.
    #[test]
    fn oracle_encode_slot_matches_extend_slot_40big() {
        let Some(clean) = std::env::var_os("NSR_CLEAN") else {
            eprintln!("skipped: NSR_CLEAN unset");
            return;
        };
        let root = PathBuf::from(&clean).join("GameCube Version").join("root");
        let mut base = root.join("pathfind.dat.bak_good_2026-07-02");
        if !base.is_file() {
            eprintln!("stock backup missing; using root/pathfind.dat");
            base = root.join("pathfind.dat");
        }
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let dir = std::env::temp_dir().join(format!("nsr-oracle-encode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("sixty.wav");
        let out = dir.join("slot21.blob");
        let (left, right) = test_signal(60);
        write_wav_24k(&wav, &left, &right);

        let py_start = std::time::Instant::now();
        let run = std::process::Command::new("py")
            .arg("-3")
            .arg(repo.join("Tools").join("oracle-encode.py"))
            .arg(&wav)
            .arg("21")
            .arg(&base)
            .arg(&out)
            .current_dir(&repo)
            .env("NSR_CLEAN", &clean)
            .output()
            .expect("py -3 runs");
        assert!(run.status.success(), "oracle failed:\n{}\n{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
        let py_secs = py_start.elapsed().as_secs_f64();
        let expected = std::fs::read(&out).unwrap();

        let bytes = std::fs::read(&base).unwrap();
        let asf = &bytes[0x13000..];
        let pts = stock_pt_headers(asf, Slot::new(21).unwrap()).unwrap();
        let rs_start = std::time::Instant::now();
        let got = encode_slot(&left, &right, &pts, &mut |_, _| {}).unwrap();
        let rs_secs = rs_start.elapsed().as_secs_f64();
        eprintln!("oracle: python {py_secs:.1}s, rust {rs_secs:.2}s, {} bytes", got.len());

        if got != expected {
            let first = got.iter().zip(&expected).position(|(a, b)| a != b).unwrap_or(got.len().min(expected.len()));
            let seg = expected.len() / 40;
            let segment = first.checked_div(seg).unwrap_or(0);
            let within = first.checked_rem(seg).unwrap_or(first);
            panic!(
                "blob differs: rust {} bytes, python {} bytes, first difference at 0x{first:X} (segment {segment}, offset 0x{within:X} in segment)",
                got.len(), expected.len()
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
