//! pathfind.dat: EA TERF with three raw sub-files; sub-file 2 is the ASF and
//! runs to end of file. The PFDx blob at 0x2000 holds the 876-entry position
//! table the streamer seeks by: w0 = SCHl offset in 128-byte units from the
//! ASF start, w1 = segment length in milliseconds.
//!
//! Resized segments starve at segment 2 when this table goes stale; that
//! was the length wall until fix_pfdx_toc.py, which `rewrite_toc` ports.
use crate::platform::{Error, Result};
use crate::soundtrack::{asf, slot_first_stream, Slot, SEGMENTS_PER_SONG, STREAM_COUNT};

const DIR1_FILE2_OFFSET: usize = 0x818;
const DIR1_FILE2_SIZE: usize = 0x81C;
const DATA_INFO_SIZE: usize = 0x1004;
const PFDX: usize = 0x2000;

fn invalid(msg: String) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

fn be32(b: &[u8], o: usize) -> Result<u32> {
    b.get(o..o + 4)
        .map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
        .ok_or_else(|| invalid(format!("pathfind.dat is shorter than 0x{o:X}")))
}

fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_be_bytes());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pathfind {
    pub asf_offset: usize,
    pub asf_size: usize,
    pub toc_offset: usize,
}

impl Pathfind {
    pub fn parse(bytes: &[u8]) -> Result<Pathfind> {
        if bytes.get(0..4) != Some(b"TERF") {
            return Err(invalid("pathfind.dat does not start with TERF".into()));
        }
        if bytes.get(PFDX..PFDX + 4) != Some(b"PFDx") {
            return Err(invalid("no PFDx blob at 0x2000".into()));
        }
        let asf_offset = 0x1000 + be32(bytes, DIR1_FILE2_OFFSET)? as usize;
        let asf_size = be32(bytes, DIR1_FILE2_SIZE)? as usize;
        let toc_offset = PFDX + be32(bytes, PFDX + 0x34)? as usize;
        if asf_offset + asf_size != bytes.len() {
            return Err(invalid(format!(
                "ASF 0x{asf_offset:X}+0x{asf_size:X} does not end at EOF 0x{:X}",
                bytes.len()
            )));
        }
        if toc_offset + 8 * STREAM_COUNT > asf_offset {
            return Err(invalid("the PFDx table overlaps the ASF".into()));
        }
        Ok(Pathfind { asf_offset, asf_size, toc_offset })
    }
}

/// The ASF sub-file, or empty when the file does not parse.
pub fn asf(bytes: &[u8]) -> &[u8] {
    match Pathfind::parse(bytes) {
        Ok(p) => &bytes[p.asf_offset..],
        Err(_) => &[],
    }
}

/// The file with a new ASF: DIR1 size and DATA INFO_SIZE move by the delta.
pub fn with_asf(bytes: &[u8], new_asf: &[u8]) -> Result<Vec<u8>> {
    let p = Pathfind::parse(bytes)?;
    let mut out = bytes[..p.asf_offset].to_vec();
    out.extend_from_slice(new_asf);
    let delta = new_asf.len() as i64 - p.asf_size as i64;
    put32(&mut out, DIR1_FILE2_SIZE, new_asf.len() as u32);
    let info = i64::from(be32(bytes, DATA_INFO_SIZE)?) + delta;
    put32(&mut out, DATA_INFO_SIZE, info as u32);
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TocReport {
    pub w0_rewritten: usize,
}

/// Rewrite every w0 from a real SCHl scan; recompute w1 for `changed` slots
/// from each segment's PT 0x85 (ms = round(samples / 24)). Unchanged
/// streams reproduce their stock values, so this is idempotent.
pub fn rewrite_toc(bytes: &mut [u8], changed: &[Slot]) -> Result<TocReport> {
    let p = Pathfind::parse(bytes)?;
    let pos = asf::scan_schl(&bytes[p.asf_offset..]);
    if pos.len() != STREAM_COUNT {
        return Err(invalid(format!(
            "{} streams, expected {STREAM_COUNT}; the layout must keep 40 segments per song",
            pos.len()
        )));
    }
    let mut rewritten = 0;
    for (k, &rel) in pos.iter().enumerate() {
        if !rel.is_multiple_of(128) {
            return Err(invalid(format!(
                "stream {k} at 0x{rel:X} is not 128-byte aligned; the engine cannot seek to it"
            )));
        }
        let o = p.toc_offset + 8 * k;
        let w0 = (rel >> 7) as u32;
        if be32(bytes, o)? != w0 {
            put32(bytes, o, w0);
            rewritten += 1;
        }
    }
    for &slot in changed {
        let first = slot_first_stream(slot);
        for (k, &rel) in pos.iter().enumerate().skip(first).take(SEGMENTS_PER_SONG) {
            let stream = &bytes[p.asf_offset + rel..];
            let samples = asf::pt_tag(asf::pt_bytes(stream), 0x85)
                .ok_or_else(|| invalid(format!("no PT 0x85 in stream {k}")))?;
            let w1 = ((samples as f64) / 24.0).round() as u32;
            put32(bytes, p.toc_offset + 8 * k + 4, w1);
        }
    }
    Ok(TocReport { w0_rewritten: rewritten })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::{synthetic_pathfind, synthetic_stream, SYN_PT};
    use crate::soundtrack::{asf, Slot};

    #[test]
    fn parses_the_terf_fields() {
        let pf = synthetic_pathfind(256);
        let p = Pathfind::parse(&pf).unwrap();
        assert_eq!(p.asf_offset, 0x13000);
        assert_eq!(p.toc_offset, 0x1138C);
        assert_eq!(p.asf_size, pf.len() - 0x13000);
        assert!(Pathfind::parse(b"not a terf").is_err());
    }

    #[test]
    fn replacing_the_asf_fixes_both_sizes_and_the_toc_follows() {
        let pf = synthetic_pathfind(256);
        let a = asf(&pf).to_vec();
        let big: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 2400, 512)).collect();
        let new_asf = asf::splice(&a, Slot::new(4).unwrap(), &big).unwrap();
        let mut out = with_asf(&pf, &new_asf).unwrap();
        let p = Pathfind::parse(&out).unwrap();
        assert_eq!(p.asf_size, new_asf.len());
        let info_before = u32::from_be_bytes(pf[0x1004..0x1008].try_into().unwrap()) as usize;
        let info_after = u32::from_be_bytes(out[0x1004..0x1008].try_into().unwrap()) as usize;
        assert_eq!(info_after - info_before, new_asf.len() - a.len());
        let rep = rewrite_toc(&mut out, &[Slot::new(4).unwrap()]).unwrap();
        assert!(rep.w0_rewritten > 0);
        // every w0 now points at a real SCHl, and slot 4's w1 is 100 ms
        let pos = asf::scan_schl(asf(&out));
        for (k, p) in pos.iter().enumerate() {
            let o = 0x1138C + 8 * k;
            let w0 = u32::from_be_bytes(out[o..o + 4].try_into().unwrap()) as usize;
            assert_eq!(w0 << 7, *p);
        }
        let k = 124; // slot 4's first stream: 44 + 2 * 40
        let w1 = u32::from_be_bytes(out[0x1138C + 8 * k + 4..0x1138C + 8 * k + 8].try_into().unwrap());
        assert_eq!(w1, 100);
        // a second pass changes nothing
        let again = rewrite_toc(&mut out, &[Slot::new(4).unwrap()]).unwrap();
        assert_eq!(again.w0_rewritten, 0);
    }

    #[test]
    fn rewrite_toc_on_stock_layout_changes_nothing() {
        let mut pf = synthetic_pathfind(256);
        let before = pf.clone();
        let rep = rewrite_toc(&mut pf, &[]).unwrap();
        assert_eq!(rep.w0_rewritten, 0);
        assert_eq!(pf, before);
    }

    #[test]
    fn oracle_stock_pathfind_parses_and_toc_is_already_right() {
        let Some(clean) = std::env::var_os("NSR_CLEAN") else {
            eprintln!("skipped: NSR_CLEAN unset");
            return;
        };
        let path = std::path::PathBuf::from(clean).join("GameCube Version").join("root").join("pathfind.dat");
        let mut bytes = std::fs::read(&path).unwrap();
        let p = Pathfind::parse(&bytes).unwrap();
        assert_eq!(p.asf_offset, 0x13000);
        assert_eq!(p.toc_offset, 0x1138C);
        let before = bytes.clone();
        let rep = rewrite_toc(&mut bytes, &[]).unwrap();
        assert_eq!(rep.w0_rewritten, 0, "stock w0 must already match the scan");
        assert_eq!(bytes, before);
    }
}
