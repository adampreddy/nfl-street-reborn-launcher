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
    // Every segment carries the same number of samples, the way the disc's
    // own do (slot 4 on the real image: 40 x 117,920). The song's tail is
    // padded with silence up to the next multiple of 40 samples, at most 39
    // of them, so no segment is shorter than the rest and the game's
    // per-segment timing holds to the end.
    let seg_samples = total.div_ceil(SEGMENTS_PER_SONG);
    let padded = seg_samples * SEGMENTS_PER_SONG;
    let (left, right): (std::borrow::Cow<'_, [i16]>, std::borrow::Cow<'_, [i16]>) = if padded > total {
        let mut l = left.to_vec();
        let mut r = right.to_vec();
        l.resize(padded, 0);
        r.resize(padded, 0);
        (l.into(), r.into())
    } else {
        (left.into(), right.into())
    };

    // Every segment encodes to the same size (fixed-rate frames, equal
    // sample counts); the first one sizes the allotment. The allotment is
    // rounded up to a 0x800 multiple with room to spare, and the rest of
    // it past SCEl is zeros, never audio.
    let trial = ea_xa::encode_stereo_stream(&left[..seg_samples], &right[..seg_samples], &pt_headers[0], None);
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
    for (k, pt_header) in pt_headers.iter().enumerate() {
        on(k, SEGMENTS_PER_SONG);
        let a = k * seg_samples;
        let b = (k + 1) * seg_samples;
        let nsamp = seg_samples;
        let pt = asf::patch_pt_tag(pt_header, 0x85, nsamp as u64).map_err(invalid)?;
        let mut st = ea_xa::encode_stereo_stream(&left[a..b], &right[a..b], &pt, Some(target));
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

    /// Run by hand against the real disc: how are the disc's own segments
    /// laid out? Per stream of one slot: the bytes from SCHl to the end of
    /// SCEl, the bytes to the next stream, PT 0x85 against the SCDl sample
    /// sum, and how many silent frames end the last block. This is what an
    /// encoded segment has to look like to play without a gap.
    ///
    ///   NSR_PRISTINE_ISO=<path> NSR_SLOT=1 cargo test -p reborn-core --lib -- --ignored stock_segment_layout --nocapture
    #[test]
    #[ignore]
    fn stock_segment_layout() {
        let Ok(iso) = std::env::var("NSR_PRISTINE_ISO") else {
            return;
        };
        let slot = std::env::var("NSR_SLOT").ok().and_then(|s| s.parse().ok()).and_then(|n| Slot::new(n).ok()).unwrap_or(Slot::new(1).unwrap());
        let disc = crate::soundtrack::gcm::Disc::open(Path::new(&iso)).unwrap();
        let pf = disc.read_file("pathfind.dat").unwrap();
        let a = crate::soundtrack::pathfind::asf(&pf);
        let pos = asf::scan_schl(a);
        let first = slot_first_stream(slot);
        let mut total_declared = 0u64;
        let mut total_bytes = 0usize;
        for k in 0..SEGMENTS_PER_SONG {
            let p = pos[first + k];
            let next = pos.get(first + k + 1).copied().unwrap_or(a.len());
            let st = &a[p..];
            let len = asf::stream_len(st, 0).unwrap_or(0);
            let pt85 = asf::pt_tag(asf::pt_bytes(st), 0x85).unwrap_or(0);
            let sum = asf::scdl_sample_sum(&st[..len]);
            // the last SCDl: how many of its per-channel frames are the silence frame
            let mut q = 8 + asf::le32(st, 4).unwrap_or(8) as usize;
            let mut last_block: Option<(usize, usize)> = None;
            let mut blocks = 0usize;
            while q + 8 <= len {
                let tag = &st[q..q + 4];
                let size = asf::le32(st, q + 4).unwrap_or(8) as usize;
                if tag == b"SCDl" {
                    last_block = Some((q, size));
                    blocks += 1;
                }
                if tag == b"SCEl" || size < 8 {
                    break;
                }
                q += size;
            }
            let silent_tail = last_block
                .map(|(q, size)| {
                    let per_ch = u32::from_be_bytes(st[q + 16..q + 20].try_into().unwrap()) as usize;
                    let data = &st[q + 20..q + 20 + per_ch.min(size.saturating_sub(20))];
                    data.chunks(15).rev().take_while(|f| f.len() == 15 && *f == crate::soundtrack::ea_xa::SILENCE_FRAME).count()
                })
                .unwrap_or(0);
            let tail_zero = a[p + len..next].iter().all(|b| *b == 0);
            eprintln!(
                "stream {k:>2}: {len:>7} bytes to SCEl, {:>7} to next (gap {:>5}, zeros {tail_zero}), blocks {blocks}, PT85 {pt85} vs SCDl {sum}, silent frames ending the last block: {silent_tail}",
                next - p,
                next - p - len
            );
            total_declared += pt85;
            total_bytes += next - p;
        }
        eprintln!("slot {}: {} declared samples = {:.1} s, {total_bytes} bytes over 40 streams", slot.get(), total_declared, total_declared as f64 / 24_000.0);
    }

    /// Run by hand against the real disc: are the 40 PT headers of every
    /// slot the same apart from the sample count? If they are, a blob
    /// encoded for one slot plays in any other, and a song can move between
    /// slots without being encoded again.
    ///
    ///   NSR_PRISTINE_ISO=<path> cargo test -p reborn-core --lib -- --ignored pt_headers_are_the_same_in_every_slot --nocapture
    #[test]
    #[ignore]
    fn pt_headers_are_the_same_in_every_slot() {
        let Ok(iso) = std::env::var("NSR_PRISTINE_ISO") else {
            return;
        };
        let disc = crate::soundtrack::gcm::Disc::open(Path::new(&iso)).unwrap();
        let pf = disc.read_file("pathfind.dat").unwrap();
        let a = crate::soundtrack::pathfind::asf(&pf);
        let mut first: Option<Vec<Vec<u8>>> = None;
        let mut differing = Vec::new();
        for slot in Slot::all() {
            let pts: Vec<Vec<u8>> = stock_pt_headers(a, slot).unwrap().iter().map(|pt| asf::patch_pt_tag(pt, 0x85, 1).unwrap()).collect();
            let lens: std::collections::BTreeSet<usize> = pts.iter().map(|p| p.len()).collect();
            let distinct: std::collections::BTreeSet<&Vec<u8>> = pts.iter().collect();
            eprintln!("slot {:>2}: {} distinct header(s) among 40 streams, lengths {:?}, first {}", slot.get(), distinct.len(), lens, hex::encode(&pts[0]));
            match &first {
                None => first = Some(pts),
                Some(f) => {
                    if *f != pts {
                        differing.push(slot.get());
                    }
                }
            }
        }
        eprintln!("slots whose headers differ from slot 1 (sample count aside): {differing:?}");
    }

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
        // Up to 39 samples of tail padding, and each segment's last frame
        // rounded up to 28 samples: about a millisecond per segment, never
        // the silent block per segment that used to be the pause between
        // them.
        assert!(total as usize - left.len() < SEGMENTS_PER_SONG * 28, "{} extra samples", total as usize - left.len());
        let d = ea_xa::decode_streams(&blob).unwrap();
        assert_eq!(d.left.len() as u64, total);
        // Every stream ends at its SCEl and the rest of its allotment is
        // zeros, like the disc's own, and every segment carries the same
        // samples.
        let seg = left.len().div_ceil(SEGMENTS_PER_SONG) as u64;
        for &p in &pos {
            let st = &blob[p..];
            let len = asf::stream_len(st, 0).unwrap();
            assert!(len <= target);
            assert!(blob[p + len..p + target].iter().all(|b| *b == 0), "zeros after SCEl");
            let sum = asf::scdl_sample_sum(&st[..len]);
            assert!(sum >= seg && sum < seg + 28, "segment declares {sum} for {seg} samples");
            assert_eq!(sum, asf::scdl_sample_sum(&blob[pos[0]..pos[0] + asf::stream_len(&blob[pos[0]..], 0).unwrap()]), "all segments alike");
        }
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

    /// Oracle 3: the Rust blob carries the same audio as extend_slot_40big.py's.
    /// The bytes differ on purpose since 2026-09-11: the Python pads each
    /// segment with silent audio blocks (a pause the game plays), this
    /// encoder pads the allotment with zeros after SCEl, like the disc.
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

        assert_eq!(got.len(), expected.len(), "the same allotment per segment");
        let ours = ea_xa::decode_streams(&got).unwrap();
        let theirs = ea_xa::decode_streams(&expected).unwrap();
        let n = left.len();
        assert!(ours.left.len() >= n && theirs.left.len() >= n);
        assert_eq!(ours.left[..n], theirs.left[..n], "the song's own samples decode the same");
        assert_eq!(ours.right[..n], theirs.right[..n]);
        assert!(ours.left.len() < theirs.left.len(), "the Python declares its silent padding as audio; this encoder does not");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
