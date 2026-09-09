//! Synthetic fixtures: a tiny disc, a tiny pathfind.dat, a tiny template.dat.
//!
//! Each one follows the real layout rules at toy size, so the byte-level
//! modules are tested without Adam's 1.46 GB disc. The oracle tests (gated on
//! `NSR_CLEAN`) cover the real files.
use std::path::{Path, PathBuf};

use crate::soundtrack::{asf, STREAM_COUNT};

pub const SMALL_FILES_START: u64 = 0x4000;

fn align(x: u64) -> u64 {
    (x + 0x7FF) & !0x7FF
}

/// A disc with the real layout rules at toy size: header 0x440, apploader at
/// 0x2440 (16 bytes), DOL at 0x2800 (a valid 0x100 header + 0x20 text), FST
/// after it, files from SMALL_FILES_START, padded to files end + 0x800.
pub fn synthetic_disc(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
    let dol_off: u64 = 0x2800;
    let mut dol = vec![0u8; 0x120];
    dol[0..4].copy_from_slice(&0x100u32.to_be_bytes()); // text0 offset
    dol[0x90..0x94].copy_from_slice(&0x20u32.to_be_bytes()); // text0 size
    let dol_len = 0x120u64;
    let fst_off = align(dol_off + dol_len);
    let total = files.len() as u32 + 1;
    let mut strtab = Vec::new();
    let mut name_off = Vec::new();
    for (n, _) in files {
        name_off.push(strtab.len() as u32);
        strtab.extend_from_slice(n.as_bytes());
        strtab.push(0);
    }
    let fst_size = 12 * u64::from(total) + strtab.len() as u64;
    let files_start = SMALL_FILES_START.max(align(fst_off + fst_size));
    let mut layout = Vec::new();
    let mut cur = files_start;
    for (_, data) in files {
        cur = align(cur);
        layout.push((cur, data.len() as u64));
        cur += data.len() as u64;
    }
    let end = align(cur) + 0x800;
    let mut fst = Vec::new();
    fst.push(1u8);
    fst.extend_from_slice(&[0, 0, 0]);
    fst.extend_from_slice(&0u32.to_be_bytes());
    fst.extend_from_slice(&total.to_be_bytes());
    for ((off, sz), noff) in layout.iter().zip(&name_off) {
        fst.push(0);
        fst.extend_from_slice(&noff.to_be_bytes()[1..]);
        fst.extend_from_slice(&(*off as u32).to_be_bytes());
        fst.extend_from_slice(&(*sz as u32).to_be_bytes());
    }
    fst.extend_from_slice(&strtab);
    let mut img = vec![0u8; end as usize];
    img[0x420..0x424].copy_from_slice(&(dol_off as u32).to_be_bytes());
    img[0x424..0x428].copy_from_slice(&(fst_off as u32).to_be_bytes());
    img[0x428..0x42C].copy_from_slice(&(fst_size as u32).to_be_bytes());
    img[0x42C..0x430].copy_from_slice(&(fst_size as u32).to_be_bytes());
    img[0x2440..0x2450].copy_from_slice(b"APPLOADER-STUB!!");
    img[dol_off as usize..(dol_off + dol_len) as usize].copy_from_slice(&dol);
    img[fst_off as usize..fst_off as usize + fst.len()].copy_from_slice(&fst);
    for ((off, _), (_, data)) in layout.iter().zip(files) {
        img[*off as usize..*off as usize + data.len()].copy_from_slice(data);
    }
    let path = dir.join("synthetic.iso");
    std::fs::write(&path, img).unwrap();
    path
}

/// A PT header shaped like the disc's: preamble with 0xFD before the tag
/// stream, then 0x80=3, 0x85 (3 bytes), 0x82=2, 0xA0=0x0A, 0x8C=4, 0xFF.
pub const SYN_PT: &[u8] = &[
    0x50, 0x54, 0x06, 0x00, 0x06, 0x01, 0x65, 0x13, 0x01, 0x7F, 0xFD, 0x80, 0x01, 0x03, 0x85, 0x03,
    0x00, 0x00, 0x00, 0x82, 0x01, 0x02, 0xA0, 0x01, 0x0A, 0x8C, 0x01, 0x04, 0xFF, 0x00,
];

/// One stream: SCHl + PT (0x85 patched to `samples`), SCCl count 1, one SCDl
/// whose data is `data_per_ch` zero bytes per channel, SCEl, padded with
/// zeros to a 128-byte multiple.
pub fn synthetic_stream(pt: &[u8], samples: u32, data_per_ch: usize) -> Vec<u8> {
    let pt = asf::patch_pt_tag(pt, 0x85, u64::from(samples)).unwrap();
    let mut s = Vec::new();
    s.extend_from_slice(b"SCHl");
    s.extend_from_slice(&((8 + pt.len()) as u32).to_le_bytes());
    s.extend_from_slice(&pt);
    s.extend_from_slice(b"SCCl");
    s.extend_from_slice(&12u32.to_le_bytes());
    s.extend_from_slice(&1u32.to_be_bytes());
    s.extend_from_slice(b"SCDl");
    s.extend_from_slice(&((20 + 2 * data_per_ch) as u32).to_le_bytes());
    s.extend_from_slice(&samples.to_be_bytes());
    s.extend_from_slice(&0u32.to_be_bytes());
    s.extend_from_slice(&(data_per_ch as u32).to_be_bytes());
    s.extend(std::iter::repeat_n(0u8, 2 * data_per_ch));
    s.extend_from_slice(b"SCEl");
    s.extend_from_slice(&8u32.to_le_bytes());
    while !s.len().is_multiple_of(128) {
        s.push(0);
    }
    s
}

/// pathfind.dat at toy size: TERF preamble, DIR1 with three entries, PFDx at
/// 0x2000 whose table pointer lands the TOC at 0x1138C, ASF at 0x13000 with
/// 876 streams of `data_per_ch` bytes per channel, TOC filled correctly.
pub fn synthetic_pathfind(data_per_ch: usize) -> Vec<u8> {
    let streams: Vec<Vec<u8>> = (0..STREAM_COUNT)
        .map(|k| synthetic_stream(SYN_PT, 2400 + k as u32 % 7, data_per_ch))
        .collect();
    let mut asf_bytes: Vec<u8> = streams.concat();
    while !asf_bytes.len().is_multiple_of(0x800) {
        asf_bytes.push(0);
    }
    let mut d = vec![0u8; 0x13000];
    d[0..4].copy_from_slice(b"TERF");
    d[0x0E..0x10].copy_from_slice(&3u16.to_be_bytes());
    // DIR1 entries {offset, size}: file0 at 0x0, file1 at 0x1000, file2 (ASF)
    // at 0x12000 -> data at 0x1000 + 0x12000 = 0x13000
    d[0x808..0x80C].copy_from_slice(&0u32.to_be_bytes());
    d[0x80C..0x810].copy_from_slice(&0x1000u32.to_be_bytes());
    d[0x810..0x814].copy_from_slice(&0x1000u32.to_be_bytes());
    d[0x814..0x818].copy_from_slice(&0x11000u32.to_be_bytes());
    d[0x818..0x81C].copy_from_slice(&0x12000u32.to_be_bytes());
    d[0x81C..0x820].copy_from_slice(&(asf_bytes.len() as u32).to_be_bytes());
    d[0x1004..0x1008].copy_from_slice(&((0x12000 + asf_bytes.len()) as u32).to_be_bytes());
    d[0x2000..0x2004].copy_from_slice(b"PFDx");
    d[0x2034..0x2038].copy_from_slice(&0xF38Cu32.to_be_bytes());
    let mut off = 0usize;
    for (k, s) in streams.iter().enumerate() {
        let o = 0x1138C + 8 * k;
        d[o..o + 4].copy_from_slice(&((off >> 7) as u32).to_be_bytes());
        let samples = asf::pt_tag(asf::pt_bytes(s), 0x85).unwrap();
        d[o + 4..o + 8].copy_from_slice(&(((samples as f64) / 24.0).round() as u32).to_be_bytes());
        off += s.len();
    }
    d.extend_from_slice(&asf_bytes);
    d
}

/// template.dat at toy size: zeros, the five EATX field definitions at
/// 0x17954 exactly as the disc has them, and 21 records at 0x1799C.
pub fn synthetic_template() -> Vec<u8> {
    let mut t = vec![0u8; 0x18700];
    let defs: [u8; 72] = [
        0x44, 0x49, 0x4E, 0x53, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20,
        0x49, 0x54, 0x4E, 0x53, 0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xB0,
        0x4C, 0x41, 0x4E, 0x53, 0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x40,
        0x52, 0x41, 0x4E, 0x53, 0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x04, 0xD0,
        0x58, 0x49, 0x4E, 0x53, 0x00, 0x00, 0x00, 0x05,
    ];
    t[0x17954..0x17954 + 72].copy_from_slice(&defs);
    for k in 0..21usize {
        let o = 0x1799C + k * 156;
        t[o..o + 4].copy_from_slice(&(k as u32 + 1).to_be_bytes());
        let put = |t: &mut Vec<u8>, at: usize, s: String| {
            t[at..at + s.len()].copy_from_slice(s.as_bytes());
        };
        put(&mut t, o + 4, format!("Song {}", k + 1));
        put(&mut t, o + 54, format!("Album {}", k + 1));
        put(&mut t, o + 104, format!("Artist {}", k + 1));
        t[o + 154] = ((k as u8) + 1) << 3;
    }
    t
}

/// A 16-bit stereo RIFF WAV at 24 kHz, for the decode and encode tests.
pub fn write_wav_24k(path: &Path, left: &[i16], right: &[i16]) {
    write_wav(path, 24_000, left, right)
}

pub fn write_wav(path: &Path, rate: u32, left: &[i16], right: &[i16]) {
    assert_eq!(left.len(), right.len());
    let data_len = (left.len() * 4) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&2u16.to_le_bytes()); // stereo
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 4).to_le_bytes());
    w.extend_from_slice(&4u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for (l, r) in left.iter().zip(right) {
        w.extend_from_slice(&l.to_le_bytes());
        w.extend_from_slice(&r.to_le_bytes());
    }
    std::fs::write(path, w).unwrap();
}

/// A format 1 `.nsmp` exactly as music_pack.py writes it (deflated entries,
/// `slots` keyed by slot number, `seconds`), so the Python oracle accepts it.
pub fn write_v1_pack(out: &Path, slot: u8, blob: &[u8], title: &str, artist: &str, album: &str) {
    use std::io::Write;
    let f = std::fs::File::create(out).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let name = format!("slot{slot}.asf");
    z.start_file(&name, opts).unwrap();
    z.write_all(blob).unwrap();
    let manifest = serde_json::json!({
        "magic": "NSMP", "format": 1, "name": "oracle", "author": "test", "description": "",
        "created": "2026-09-09", "game": "NFL Street 2 (GC)",
        "slots": { slot.to_string(): { "title": title, "artist": artist, "album": album,
                                       "blob": name, "bytes": blob.len(), "seconds": 0 } }
    });
    z.start_file("pack.json", opts).unwrap();
    z.write_all(manifest.to_string().as_bytes()).unwrap();
    z.finish().unwrap();
}
