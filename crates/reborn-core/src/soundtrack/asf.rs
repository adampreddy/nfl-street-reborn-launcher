//! EA SCHl ASF streams: pure concatenation of SCHl..SCCl..SCDl*..SCEl, no
//! table of contents. Slot s owns 40 consecutive streams (see
//! `slot_first_stream`). Every SCHl on the disc sits on a 128-byte boundary
//! because the PFDx table stores positions in 128-byte units.
use crate::platform::{Error, Result};
use crate::soundtrack::{slot_first_stream, Slot, SAMPLE_RATE, SEGMENTS_PER_SONG, STREAM_COUNT};

fn invalid(msg: String) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

/// Offsets of every `SCHl` tag. A tag inside ADPCM data is possible in
/// theory; the shipped file has none (876 real streams), and the count check
/// in `positions_checked` would catch one.
pub fn scan_schl(asf: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= asf.len() {
        match asf[i..].windows(4).position(|w| w == b"SCHl") {
            Some(p) => {
                out.push(i + p);
                i += p + 4;
            }
            None => break,
        }
    }
    out
}

pub(crate) fn le32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

pub(crate) fn be32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
}

/// Bytes from `start` through the SCEl chunk, walking chunk sizes.
pub fn stream_len(asf: &[u8], start: usize) -> Option<usize> {
    let mut p = start;
    loop {
        let tag = asf.get(p..p + 4)?;
        let size = le32(asf, p + 4)? as usize;
        if size < 8 {
            return None;
        }
        p = p.checked_add(size)?;
        if tag == b"SCEl" {
            return Some(p - start);
        }
    }
}

/// `[start, end)` of a slot's 40 streams within the ASF.
pub fn slot_range(positions: &[usize], asf_len: usize, slot: Slot) -> (usize, usize) {
    let first = slot_first_stream(slot);
    let end = first + SEGMENTS_PER_SONG;
    (positions[first], if end < positions.len() { positions[end] } else { asf_len })
}

fn positions_checked(asf: &[u8]) -> Result<Vec<usize>> {
    let pos = scan_schl(asf);
    if pos.len() != STREAM_COUNT {
        return Err(invalid(format!(
            "the ASF has {} streams, expected {STREAM_COUNT}; the layout must keep 40 segments per song",
            pos.len()
        )));
    }
    Ok(pos)
}

/// The 40-stream blob a slot holds now.
pub fn slot_blob(asf: &[u8], slot: Slot) -> Result<Vec<u8>> {
    let pos = positions_checked(asf)?;
    let (a, b) = slot_range(&pos, asf.len(), slot);
    Ok(asf[a..b].to_vec())
}

/// Replace a slot's 40 streams; the result is padded to a 0x800 multiple.
pub fn splice(asf: &[u8], slot: Slot, blob: &[u8]) -> Result<Vec<u8>> {
    validate_blob(blob).map_err(invalid)?;
    let pos = positions_checked(asf)?;
    let (a, b) = slot_range(&pos, asf.len(), slot);
    let mut out = Vec::with_capacity(asf.len() + blob.len());
    out.extend_from_slice(&asf[..a]);
    out.extend_from_slice(blob);
    out.extend_from_slice(&asf[b..]);
    while !out.len().is_multiple_of(0x800) {
        out.push(0);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobInfo {
    pub streams: usize,
    pub samples: u64,
    pub duration_ms: u64,
}

/// A game-ready slot blob: exactly 40 streams, each starting on a 128-byte
/// boundary of the blob, each closed by SCEl, each with a PT 0x85 count.
pub fn validate_blob(blob: &[u8]) -> std::result::Result<BlobInfo, String> {
    let pos = scan_schl(blob);
    if pos.len() != SEGMENTS_PER_SONG {
        return Err(format!("{} streams, expected {SEGMENTS_PER_SONG}", pos.len()));
    }
    if pos[0] != 0 {
        return Err("the blob does not start with SCHl".into());
    }
    let mut samples = 0u64;
    for (k, &p) in pos.iter().enumerate() {
        if !p.is_multiple_of(128) {
            return Err(format!("stream {k} starts at 0x{p:X}, not 128-byte aligned"));
        }
        let len = stream_len(blob, p).ok_or_else(|| format!("stream {k} has no SCEl"))?;
        let next = pos.get(k + 1).copied().unwrap_or(blob.len());
        if p + len > next {
            return Err(format!("stream {k} runs into stream {}", k + 1));
        }
        samples += pt_tag(pt_bytes(&blob[p..]), 0x85)
            .ok_or_else(|| format!("stream {k} has no PT 0x85 sample count"))?;
    }
    Ok(BlobInfo { streams: pos.len(), samples, duration_ms: samples * 1000 / u64::from(SAMPLE_RATE) })
}

/// The PT payload of a stream (after the 8-byte SCHl header).
pub fn pt_bytes(stream: &[u8]) -> &[u8] {
    let size = le32(stream, 4).unwrap_or(8) as usize;
    let end = size.clamp(8, stream.len().max(8));
    &stream[8.min(stream.len())..end.min(stream.len())]
}

/// The tag stream begins at the first 0x80 byte; the preamble can contain
/// 0xFD, which must not be parsed as a tag.
fn tag_start(pt: &[u8]) -> Option<usize> {
    pt.iter().position(|&b| b == 0x80)
}

/// Read a PT tag's big-endian value.
pub fn pt_tag(pt: &[u8], tag: u8) -> Option<u64> {
    let mut i = tag_start(pt)?;
    while i + 1 < pt.len() {
        let t = pt[i];
        if t == 0xFF {
            return None;
        }
        let ln = pt[i + 1] as usize;
        if t == tag {
            let v = pt.get(i + 2..i + 2 + ln)?;
            return Some(v.iter().fold(0u64, |acc, &b| (acc << 8) | u64::from(b)));
        }
        i += 2 + ln;
    }
    None
}

/// Write a PT tag's value in place, keeping the field length.
pub fn patch_pt_tag(pt: &[u8], tag: u8, value: u64) -> std::result::Result<Vec<u8>, String> {
    let mut out = pt.to_vec();
    let mut i = tag_start(pt).ok_or("no tag stream in PT")?;
    while i + 1 < out.len() {
        let t = out[i];
        if t == 0xFF {
            break;
        }
        let ln = out[i + 1] as usize;
        if t == tag {
            if ln == 0 || ln > 8 || (ln < 8 && value >= 1u64 << (8 * ln)) {
                return Err(format!("value {value} does not fit a {ln}-byte field"));
            }
            if out.len() < i + 2 + ln {
                return Err("PT tag runs past the header".into());
            }
            let bytes = value.to_be_bytes();
            out[i + 2..i + 2 + ln].copy_from_slice(&bytes[8 - ln..]);
            return Ok(out);
        }
        i += 2 + ln;
    }
    Err(format!("no 0x{tag:02X} field in PT"))
}

/// Sum of the SCDl per-block sample fields (the engine credits these
/// against PT 0x85, so the two must agree exactly).
pub fn scdl_sample_sum(stream: &[u8]) -> u64 {
    let mut p = 0;
    let mut total = 0u64;
    while p + 8 <= stream.len() {
        let tag = &stream[p..p + 4];
        let Some(size) = le32(stream, p + 4) else { break };
        if size < 8 {
            break;
        }
        if tag == b"SCDl" {
            total += u64::from(be32(stream, p + 8).unwrap_or(0));
        }
        if tag == b"SCEl" {
            break;
        }
        p += size as usize;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::{synthetic_pathfind, synthetic_stream, SYN_PT};
    use crate::soundtrack::{pathfind, Slot, STREAM_COUNT};

    #[test]
    fn scans_every_stream_and_finds_slot_ranges() {
        let pf = synthetic_pathfind(256);
        let a = pathfind::asf(&pf);
        let pos = scan_schl(a);
        assert_eq!(pos.len(), STREAM_COUNT);
        assert!(pos.iter().all(|p| p % 128 == 0));
        let (s, e) = slot_range(&pos, a.len(), Slot::new(1).unwrap());
        assert_eq!(s, pos[3]);
        assert_eq!(e, pos[43]);
        let (s, e) = slot_range(&pos, a.len(), Slot::new(21).unwrap());
        assert_eq!(s, pos[804]);
        assert_eq!(e, pos[844]);
    }

    #[test]
    fn a_slot_blob_validates_and_splices_back_unchanged() {
        let pf = synthetic_pathfind(256);
        let a = pathfind::asf(&pf);
        let blob = slot_blob(a, Slot::new(5).unwrap()).unwrap();
        let info = validate_blob(&blob).unwrap();
        assert_eq!(info.streams, 40);
        assert_eq!(splice(a, Slot::new(5).unwrap(), &blob).unwrap(), a);
    }

    #[test]
    fn splicing_a_bigger_blob_grows_and_pads() {
        let pf = synthetic_pathfind(256);
        let a = pathfind::asf(&pf);
        let big: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 1000, 512)).collect();
        let out = splice(a, Slot::new(2).unwrap(), &big).unwrap();
        assert_eq!(out.len() % 0x800, 0);
        assert!(out.len() > a.len());
        assert_eq!(scan_schl(&out).len(), STREAM_COUNT);
        assert_eq!(slot_blob(&out, Slot::new(2).unwrap()).unwrap(), big);
        assert_eq!(
            slot_blob(&out, Slot::new(3).unwrap()).unwrap(),
            slot_blob(a, Slot::new(3).unwrap()).unwrap()
        );
        assert_eq!(
            slot_blob(&out, Slot::new(21).unwrap()).unwrap(),
            slot_blob(a, Slot::new(21).unwrap()).unwrap()
        );
    }

    #[test]
    fn pt_tags_read_and_patch_in_place() {
        let s = synthetic_stream(SYN_PT, 1234, 256);
        let pt = pt_bytes(&s);
        assert_eq!(pt_tag(pt, 0x85), Some(1234));
        assert_eq!(pt_tag(pt, 0x82), Some(2));
        assert_eq!(pt_tag(pt, 0x84), None, "no sample rate tag on the disc");
        let p2 = patch_pt_tag(pt, 0x85, 99).unwrap();
        assert_eq!(p2.len(), pt.len());
        assert_eq!(pt_tag(&p2, 0x85), Some(99));
        assert!(patch_pt_tag(pt, 0x85, 1 << 24).is_err(), "3-byte field");
        assert_eq!(scdl_sample_sum(&s), 1234);
        assert!(stream_len(&s, 0).unwrap() <= s.len());
    }

    #[test]
    fn validate_rejects_wrong_stream_counts_and_misalignment() {
        let bad: Vec<u8> = (0..39).flat_map(|_| synthetic_stream(SYN_PT, 10, 128)).collect();
        assert!(validate_blob(&bad).unwrap_err().contains("39"));
        let mut shifted = vec![0u8; 64];
        shifted.extend((0..40).flat_map(|_| synthetic_stream(SYN_PT, 10, 128)));
        assert!(validate_blob(&shifted).unwrap_err().contains("start with SCHl"));
    }
}
