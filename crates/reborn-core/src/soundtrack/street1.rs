//! Build the NFL Street 1 pack from extracted disc files, and the two
//! soundtrack package folders the mod browser lists.
//!
//! NFL Street 1's `femusic.dat` is a TERF holding one full-length EA-XA
//! stream per song (14 songs, 32 kHz stereo, PT 0x84 = 32000), and its
//! `template.dat` carries an EATX banner table of the same family as NFL
//! Street 2's but a different shape: 32-byte title and album, 33-byte artist,
//! 104-byte records, SNID an arbitrary song id and the 5-bit SNIX running
//! 1..14 in record order. Song k (SNIX k + 1) is paired with stream k and
//! goes to NFL Street 2 slot k + 1, encoded with that slot's stock PT headers.
//!
//! NFL Street 1's SCDl blocks are the EA-XA v1 shape: each channel's region
//! starts with two big-endian 16-bit history samples, then plain 15-byte
//! ADPCM frames, no 0xEE PCM frames. NFL Street 2 uses the v2 shape the
//! shared `ea_xa` decoder reads, so this module decodes v1 itself.
//!
//! The NFL Street 1 PT header has no 0x80 tag: its tag stream starts right
//! after the 0xFD preamble byte (`85 03 .. 82 01 02 84 02 7d 00 ..`), which the
//! shared `asf::pt_tag` (anchored on 0x80) cannot see. This module reads the
//! header itself for the sample rate and sample count.
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::platform::{Error, Result};
use crate::soundtrack::{asf, pack, pathfind, resample, segments, Slot, SAMPLE_RATE};

/// Songs on the NFL Street 1 disc.
pub const SONG_COUNT: usize = 14;
pub const PACK_NAME: &str = "NFL Street Soundtrack";
pub const PACK_AUTHOR: &str = "NFL Street Reborn";
pub const PACK_DESCRIPTION: &str = "The 14 songs from the first NFL Street, in slots 1 to 14.";

pub const ORIGINAL_ID: &str = "reborn.soundtrack.original";
pub const ORIGINAL_NAME: &str = "NFL Street 2 Original Soundtrack";
pub const ORIGINAL_DESCRIPTION: &str = "Every slot plays the song the disc shipped with.";
/// The original package's payload: a pack with no songs, which means "every
/// slot is the disc's own".
pub const ORIGINAL_PACK_JSON: &str =
    r#"{"magic":"NSRMUSIC","format":2,"id":"original","name":"NFL Street 2 Original Soundtrack","songs":{}}"#;

pub const STREET1_ID: &str = "reborn.soundtrack.nfl-street-1";
pub const STREET1_NAME: &str = "NFL Street 1 Soundtrack";
pub const STREET1_DESCRIPTION: &str =
    "The 14 songs from the first NFL Street, in slots 1 to 14. Slots 15 to 21 keep the NFL Street 2 songs.";
pub const STREET1_PACK_FILE: &str = "nfl-street-1.nsrmusic";

fn invalid(msg: String) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.to_path_buf(), source }
}

fn be32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
}

// ---------------------------------------------------------------------------
// The NFL Street 1 PT header
// ---------------------------------------------------------------------------

/// A PT tag's value, with the tag stream taken to start after the 0xFD
/// preamble byte. Works for both discs' headers (NFL Street 2's `FD 80 ..`
/// and NFL Street 1's `FD 85 ..`).
pub fn pt_tag_after_fd(pt: &[u8], tag: u8) -> Option<u64> {
    let mut i = pt.iter().position(|&b| b == 0xFD)? + 1;
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

/// Sample rate, channels, and sample count of a stream's PT header.
pub fn stream_format(stream: &[u8]) -> (u32, u32, u64) {
    let pt = asf::pt_bytes(stream);
    (
        pt_tag_after_fd(pt, 0x84).map(|v| v as u32).unwrap_or(SAMPLE_RATE),
        pt_tag_after_fd(pt, 0x82).map(|v| v as u32).unwrap_or(2),
        pt_tag_after_fd(pt, 0x85).unwrap_or(0),
    )
}

// ---------------------------------------------------------------------------
// The NFL Street 1 stream shape (EA-XA v1 blocks)
// ---------------------------------------------------------------------------

const COEF1: [i32; 4] = [0, 240, 460, 392];
const COEF2: [i32; 4] = [0, 0, -208, -220];

/// A decoded NFL Street 1 stream, with the layout proof the tests check.
///
/// Each block's two history words are the encoder's exact PCM reference for
/// the previous two samples, not the lossy decoder output, so they never
/// equal the decoded samples exactly. The honest proof of the block shape
/// and the history order is that the step between the last sample of one
/// block and the first of the next (`boundary_step_mean`, left channel) is
/// no larger than the ordinary sample-to-sample step inside blocks
/// (`inner_step_mean`). Measured on the disc: 1966 versus 1935; the swapped
/// history order gives 3114 and clips.
#[derive(Debug, Clone, PartialEq)]
pub struct Street1Decoded {
    pub left: Vec<i16>,
    pub right: Vec<i16>,
    pub sample_rate: u32,
    pub blocks: usize,
    pub boundary_step_mean: f64,
    pub inner_step_mean: f64,
}

fn decode_v1_frame(frame: &[u8], h1: &mut i32, h2: &mut i32, out: &mut Vec<i16>, want: usize) -> Result<()> {
    let b0 = frame[0];
    let coef = usize::from(b0 >> 4);
    let shift = u32::from(b0 & 0x0F) + 8;
    if coef > 3 {
        return Err(invalid(format!("frame byte 0x{b0:02X} names coefficient set {coef}")));
    }
    for i in 0..28 {
        if out.len() >= want {
            break;
        }
        let byte = frame[1 + i / 2];
        let nib = u32::from(if i % 2 == 0 { byte >> 4 } else { byte & 0x0F });
        let val = ((nib << 28) as i32) >> shift;
        let sample = ((val + COEF1[coef] * *h1 + COEF2[coef] * *h2) >> 8).clamp(-32768, 32767);
        *h2 = *h1;
        *h1 = sample;
        out.push(sample as i16);
    }
    Ok(())
}

/// Decode one NFL Street 1 stream (SCHl .. SCEl) with the v1 block shape.
pub fn decode_street1_stream(stream: &[u8]) -> Result<Street1Decoded> {
    if stream.get(0..4) != Some(b"SCHl") {
        return Err(invalid("stream does not start with SCHl".into()));
    }
    let (sample_rate, channels, _) = stream_format(stream);
    if channels != 2 {
        return Err(invalid(format!("NFL Street 1 stream has {channels} channels, expected 2")));
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut blocks = 0;
    let mut boundary_steps = 0u64;
    let mut block_starts: Vec<usize> = Vec::new();
    let mut p = 0;
    while p + 8 <= stream.len() {
        let tag = &stream[p..p + 4];
        let size = u32::from_le_bytes([stream[p + 4], stream[p + 5], stream[p + 6], stream[p + 7]]) as usize;
        if size < 8 {
            return Err(invalid(format!("chunk at 0x{p:X} has size {size}")));
        }
        if tag == b"SCEl" {
            break;
        }
        if tag == b"SCDl" {
            let samples = be32(stream, p + 8).ok_or_else(|| invalid("short SCDl".into()))? as usize;
            let bpc = be32(stream, p + 16).ok_or_else(|| invalid("short SCDl".into()))? as usize;
            let data = stream.get(p + 20..p + size).ok_or_else(|| invalid("SCDl runs past the stream".into()))?;
            if data.len() < 2 * bpc || bpc < 4 {
                return Err(invalid(format!("block {blocks}: {} data bytes for 2 x {bpc}", data.len())));
            }
            for (ch, out) in [&mut left, &mut right].into_iter().enumerate() {
                let region = &data[ch * bpc..(ch + 1) * bpc];
                // The first word is the newest history sample, the second the
                // one before it (the swapped order measurably breaks decoding).
                let mut h1 = i32::from(i16::from_be_bytes([region[0], region[1]]));
                let mut h2 = i32::from(i16::from_be_bytes([region[2], region[3]]));
                let start = out.len();
                let want = start + samples;
                let mut q = 4;
                while out.len() < want {
                    let frame = region.get(q..q + 15).ok_or_else(|| invalid(format!("block {blocks} channel {ch} ends inside a frame")))?;
                    decode_v1_frame(frame, &mut h1, &mut h2, out, want)
                        .map_err(|e| invalid(format!("block {blocks} channel {ch}: {e}")))?;
                    q += 15;
                }
                if ch == 0 {
                    if blocks > 0 && start > 0 && start < out.len() {
                        boundary_steps += u64::from((i32::from(out[start]) - i32::from(out[start - 1])).unsigned_abs());
                    }
                    block_starts.push(start);
                }
            }
            blocks += 1;
        }
        p += size;
    }
    if blocks == 0 {
        return Err(invalid("no SCDl block in the stream".into()));
    }
    let total_steps: u64 = left.windows(2).map(|w| u64::from((i32::from(w[1]) - i32::from(w[0])).unsigned_abs())).sum();
    let boundaries = block_starts.len().saturating_sub(1) as u64;
    let inner_count = (left.len() as u64).saturating_sub(1).saturating_sub(boundaries).max(1);
    let boundary_step_mean = if boundaries == 0 { 0.0 } else { boundary_steps as f64 / boundaries as f64 };
    let inner_step_mean = (total_steps - boundary_steps) as f64 / inner_count as f64;
    Ok(Street1Decoded { left, right, sample_rate, blocks, boundary_step_mean, inner_step_mean })
}

// ---------------------------------------------------------------------------
// The NFL Street 1 banner table
// ---------------------------------------------------------------------------

/// One row of NFL Street 1's EATX table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Street1Record {
    /// The song id the playlist refers to. Not sequential on this disc.
    pub snid: u32,
    /// The 5-bit song index, 1..14 in record order.
    pub snix: u8,
    pub title: String,
    pub album: String,
    pub artist: String,
}

fn read_str(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}

/// Read the 14 records by their own field definitions: the five tags
/// (DINS, ITNS, LANS, RANS, XINS) give each field's bit width and end bit,
/// and the record stride is the total rounded up to 4 bytes (104 here,
/// 156 on the NFL Street 2 disc).
pub fn read_street1_records(template: &[u8]) -> Result<Vec<Street1Record>> {
    const TAGS: [&[u8; 4]; 5] = [b"DINS", b"ITNS", b"LANS", b"RANS", b"XINS"];
    let mut i = 0;
    let base = loop {
        let Some(p) = template.get(i..).and_then(|t| t.windows(4).position(|w| w == b"DINS")) else {
            return Err(invalid("the EATX music table was not found in NFL Street 1's template.dat".into()));
        };
        let base = i + p;
        if TAGS.iter().enumerate().all(|(k, tag)| template.get(base + 16 * k..base + 16 * k + 4) == Some(&tag[..])) {
            break base;
        }
        i = base + 4;
    };
    // (offset bytes, length bytes) for SNID, title, album, artist; SNIX bits.
    let mut fields = Vec::with_capacity(4);
    for k in 0..4 {
        let d = base + 16 * k;
        let bits = be32(template, d + 4).ok_or_else(|| invalid("short field definition".into()))? as usize;
        let end = be32(template, d + 12).ok_or_else(|| invalid("short field definition".into()))? as usize;
        if bits == 0 || end < bits || !bits.is_multiple_of(8) || !(end - bits).is_multiple_of(8) {
            return Err(invalid(format!("field {k} of the EATX table is not byte aligned ({bits} bits ending at {end})")));
        }
        fields.push(((end - bits) / 8, bits / 8));
    }
    let snix_bits = be32(template, base + 64 + 4).ok_or_else(|| invalid("short SNIX definition".into()))? as usize;
    let last_end_bits = be32(template, base + 48 + 12).ok_or_else(|| invalid("short field definition".into()))? as usize;
    let total_bits = last_end_bits + snix_bits;
    let stride = total_bits.div_ceil(8).div_ceil(4) * 4;
    let rec0 = base + 72;
    if template.len() < rec0 + SONG_COUNT * stride {
        return Err(invalid("NFL Street 1's template.dat ends inside the EATX table".into()));
    }
    let snix_byte = last_end_bits / 8;
    let mut out = Vec::with_capacity(SONG_COUNT);
    for k in 0..SONG_COUNT {
        let r = &template[rec0 + k * stride..rec0 + (k + 1) * stride];
        let (title_off, title_len) = fields[1];
        let (album_off, album_len) = fields[2];
        let (artist_off, artist_len) = fields[3];
        out.push(Street1Record {
            snid: be32(r, fields[0].0).unwrap_or(0),
            snix: r[snix_byte] >> (8 - snix_bits),
            title: read_str(&r[title_off..title_off + title_len]),
            album: read_str(&r[album_off..album_off + album_len]),
            artist: read_str(&r[artist_off..artist_off + artist_len]),
        });
    }
    for (k, rec) in out.iter().enumerate() {
        if usize::from(rec.snix) != k + 1 {
            return Err(invalid(format!(
                "NFL Street 1 record {k} has SNIX {} rather than {}; the stream pairing assumes record order",
                rec.snix,
                k + 1
            )));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Songs
// ---------------------------------------------------------------------------

/// One NFL Street 1 song: its whole stream and its banner text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Street1Song {
    /// 0-based stream index, so slot = index + 1.
    pub index: usize,
    pub stream: Vec<u8>,
    pub snid: u32,
    pub snix: u8,
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// Read the 14 songs out of an extracted NFL Street 1 `files` directory.
/// Stream k is paired with the record whose SNIX is k + 1 (record k).
pub fn read_street1(files_dir: &Path) -> Result<Vec<Street1Song>> {
    let fe_path = files_dir.join("femusic.dat");
    let fe = std::fs::read(&fe_path).map_err(|e| io(&fe_path, e))?;
    let positions = asf::scan_schl(&fe);
    if positions.len() != SONG_COUNT {
        return Err(invalid(format!(
            "{} has {} SCHl streams, expected {SONG_COUNT}; is this the NFL Street 1 disc?",
            fe_path.display(),
            positions.len()
        )));
    }
    let tpl_path = files_dir.join("template.dat");
    let template = std::fs::read(&tpl_path).map_err(|e| io(&tpl_path, e))?;
    let records = read_street1_records(&template)?;
    let mut songs = Vec::with_capacity(SONG_COUNT);
    for (index, (&start, record)) in positions.iter().zip(&records).enumerate() {
        let len = asf::stream_len(&fe, start)
            .ok_or_else(|| invalid(format!("stream {index} at 0x{start:X} in femusic.dat has no SCEl")))?;
        songs.push(Street1Song {
            index,
            stream: fe[start..start + len].to_vec(),
            snid: record.snid,
            snix: record.snix,
            title: record.title.clone(),
            artist: record.artist.clone(),
            album: record.album.clone(),
        });
    }
    Ok(songs)
}

/// What building the pack did.
#[derive(Debug, Clone)]
pub struct BuildReport {
    /// (slot, title, artist, duration_ms) of every song in the pack.
    pub songs: Vec<(u8, String, String, u64)>,
    /// (slot, title, reason) of every song left out. A song is left out
    /// whole, never truncated.
    pub skipped: Vec<(u8, String, String)>,
    pub manifest: pack::Manifest,
}

/// Decode, resample, and encode every NFL Street 1 song into its slot, then
/// write the pack. `on(k, 14)` is called before song k.
pub fn build_pack(
    files_dir: &Path,
    pristine_pathfind: &[u8],
    out: &Path,
    on: &mut dyn FnMut(usize, usize),
) -> Result<BuildReport> {
    let songs = read_street1(files_dir)?;
    let stock_asf = pathfind::asf(pristine_pathfind);
    if stock_asf.is_empty() {
        pathfind::Pathfind::parse(pristine_pathfind)?;
    }
    let mut inputs = Vec::with_capacity(SONG_COUNT);
    let mut report_songs = Vec::with_capacity(SONG_COUNT);
    let mut skipped = Vec::new();
    for song in &songs {
        on(song.index, SONG_COUNT);
        let slot = Slot::new(song.index as u8 + 1)?;
        let decoded = decode_street1_stream(&song.stream)?;
        let left: Vec<f32> = decoded.left.iter().map(|&s| f32::from(s) / 32768.0).collect();
        let right: Vec<f32> = decoded.right.iter().map(|&s| f32::from(s) / 32768.0).collect();
        let (l24, r24) = resample::to_24k(&[left, right], decoded.sample_rate);
        let pts = segments::stock_pt_headers(stock_asf, slot)?;
        match segments::encode_slot(&l24, &r24, &pts, &mut |_, _| {}) {
            Ok(blob) => {
                let duration_ms = l24.len() as u64 * 1000 / u64::from(SAMPLE_RATE);
                report_songs.push((slot.get(), song.title.clone(), song.artist.clone(), duration_ms));
                inputs.push(pack::SongInput {
                    slot,
                    blob,
                    title: song.title.clone(),
                    artist: song.artist.clone(),
                    album: song.album.clone(),
                });
            }
            Err(e) if e.to_string().contains("fit a slot") => {
                skipped.push((slot.get(), song.title.clone(), e.to_string()));
            }
            Err(e) => return Err(e),
        }
    }
    let manifest = pack::write_pack(out, PACK_NAME, PACK_AUTHOR, PACK_DESCRIPTION, &inputs)?;
    Ok(BuildReport { songs: report_songs, skipped, manifest })
}

// ---------------------------------------------------------------------------
// The two package folders
// ---------------------------------------------------------------------------

/// A `nsr.mod.v1` manifest for a soundtrack package, fields in the order the
/// plan fixes. Built by hand so the order survives (serde_json sorts keys).
fn package_manifest(id: &str, name: &str, version: &str, description: &str, files: &[(&str, String, u64)]) -> String {
    let q = |s: &str| serde_json::to_string(s).expect("a string serializes");
    let entries: Vec<String> = files
        .iter()
        .map(|(path, sha, bytes)| {
            format!("    {{\n      \"path\": {},\n      \"sha256\": {},\n      \"bytes\": {bytes}\n    }}", q(path), q(sha))
        })
        .collect();
    format!(
        "{{\n  \"schema\": \"nsr.soundtrack-mod.v1\",\n  \"magic\": \"NSST\",\n  \"id\": {},\n  \"name\": {},\n  \"version\": {},\n  \"targets\": [],\n  \"conflict_group\": \"soundtrack\",\n  \"kind\": \"soundtrack\",\n  \"category\": \"music\",\n  \"description\": {},\n  \"files\": [\n{}\n  ]\n}}\n",
        q(id),
        q(name),
        q(version),
        q(description),
        entries.join(",\n")
    )
}

fn write_package(out_dir: &Path, id: &str, name: &str, version: &str, description: &str, payload_name: &str, payload: &[u8]) -> Result<()> {
    let dir = out_dir.join(id).join(version);
    let files = dir.join("files");
    std::fs::create_dir_all(&files).map_err(|e| io(&files, e))?;
    let payload_path = files.join(payload_name);
    std::fs::write(&payload_path, payload).map_err(|e| io(&payload_path, e))?;
    let sha = hex::encode(Sha256::digest(payload));
    let manifest = package_manifest(id, name, version, description, &[(payload_name, sha, payload.len() as u64)]);
    let manifest_path = dir.join("package.json");
    std::fs::write(&manifest_path, manifest).map_err(|e| io(&manifest_path, e))
}

/// Write the two soundtrack package folders under `out_dir`:
/// `reborn.soundtrack.original/<version>` (a pack with no songs) and
/// `reborn.soundtrack.nfl-street-1/<version>` (a copy of `pack`).
pub fn write_packages(pack_path: &Path, out_dir: &Path, version: &str) -> Result<()> {
    let pack_bytes = std::fs::read(pack_path).map_err(|e| io(pack_path, e))?;
    pack::read_pack(pack_path)?;
    write_package(out_dir, ORIGINAL_ID, ORIGINAL_NAME, version, ORIGINAL_DESCRIPTION, "pack.json", ORIGINAL_PACK_JSON.as_bytes())?;
    write_package(out_dir, STREET1_ID, STREET1_NAME, version, STREET1_DESCRIPTION, STREET1_PACK_FILE, &pack_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::{synthetic_stream, SYN_PT};
    use std::path::PathBuf;

    fn small_pack(dir: &Path) -> PathBuf {
        let blob: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 2400, 256)).collect();
        let out = dir.join("small.nsrmusic");
        pack::write_pack(
            &out,
            "Small",
            "test",
            "",
            &[pack::SongInput { slot: Slot::new(1).unwrap(), blob, title: "One".into(), artist: "A".into(), album: "L".into() }],
        )
        .unwrap();
        out
    }

    #[test]
    fn packages_are_written_with_hashes_in_the_fixed_field_order() {
        let dir = tempfile::tempdir().unwrap();
        let pack_path = small_pack(dir.path());
        let out = dir.path().join("packages");
        write_packages(&pack_path, &out, "1.0.0").unwrap();

        let orig_dir = out.join(ORIGINAL_ID).join("1.0.0");
        let s1_dir = out.join(STREET1_ID).join("1.0.0");
        let orig_payload = std::fs::read(orig_dir.join("files").join("pack.json")).unwrap();
        assert_eq!(orig_payload, ORIGINAL_PACK_JSON.as_bytes());
        let s1_payload = std::fs::read(s1_dir.join("files").join(STREET1_PACK_FILE)).unwrap();
        assert_eq!(s1_payload, std::fs::read(&pack_path).unwrap(), "the pack is copied verbatim");
        pack::read_pack(&s1_dir.join("files").join(STREET1_PACK_FILE)).unwrap();

        for (dir, id, name, payload_name, payload) in [
            (&orig_dir, ORIGINAL_ID, ORIGINAL_NAME, "pack.json", &orig_payload),
            (&s1_dir, STREET1_ID, STREET1_NAME, STREET1_PACK_FILE, &s1_payload),
        ] {
            let raw = std::fs::read_to_string(dir.join("package.json")).unwrap();
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v["schema"], "nsr.soundtrack-mod.v1");
            assert_eq!(v["magic"], "NSST");
            assert_eq!(v["id"], id);
            assert_eq!(v["name"], name);
            assert_eq!(v["version"], "1.0.0");
            assert_eq!(v["targets"], serde_json::json!([]));
            assert_eq!(v["conflict_group"], "soundtrack");
            assert_eq!(v["kind"], "soundtrack");
            assert_eq!(v["category"], "music");
            assert!(v["description"].as_str().unwrap().len() > 10);
            assert_eq!(v["files"][0]["path"], payload_name);
            assert_eq!(v["files"][0]["sha256"], hex::encode(Sha256::digest(payload)));
            assert_eq!(v["files"][0]["bytes"], payload.len() as u64);
            // the field order the plan fixes
            let order = ["\"schema\"", "\"magic\"", "\"id\"", "\"name\"", "\"version\"", "\"targets\"", "\"conflict_group\"", "\"kind\"", "\"category\"", "\"description\"", "\"files\""];
            let idx: Vec<usize> = order.iter().map(|k| raw.find(k).unwrap()).collect();
            assert!(idx.windows(2).all(|w| w[0] < w[1]), "field order {idx:?}");
        }

        // What the mod library's loader says about these manifests today. It
        // wants every payload path to be <gameid>/<name>.dds|.png and refuses
        // an empty `targets` list, so a soundtrack package is not a texture
        // package to it. Pinned here so the fact is visible; the parent
        // decides how the mods page admits soundtrack packages.
        let err = crate::mods::ModPackage::load(&orig_dir).unwrap_err().to_string();
        eprintln!("ModPackage::load(original): {err}");
        let err2 = crate::mods::ModPackage::load(&s1_dir).unwrap_err().to_string();
        eprintln!("ModPackage::load(nfl-street-1): {err2}");
        assert!(err.contains("payload path") || err.contains("target"), "{err}");
        assert!(err2.contains("payload path") || err2.contains("target"), "{err2}");
    }

    #[test]
    fn read_street1_refuses_a_directory_without_the_disc_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_street1(dir.path()).is_err());
        std::fs::write(dir.path().join("femusic.dat"), b"TERF but no streams").unwrap();
        let err = read_street1(dir.path()).unwrap_err().to_string();
        assert!(err.contains("expected 14"), "{err}");
    }

    #[test]
    fn pt_tags_are_read_after_the_fd_byte_on_both_header_shapes() {
        // NFL Street 2 shape: FD 80 01 03 85 .. 82 01 02 ..
        assert_eq!(pt_tag_after_fd(SYN_PT, 0x82), Some(2));
        assert_eq!(pt_tag_after_fd(SYN_PT, 0x84), None);
        // NFL Street 1 shape, as femusic.dat stream 0 has it
        let ns1: &[u8] = &[
            0x50, 0x54, 0x06, 0x00, 0x06, 0x01, 0x65, 0x0B, 0x01, 0x02, 0xFD, 0x85, 0x03, 0x47, 0x46, 0x87, 0x82, 0x01, 0x02,
            0x84, 0x02, 0x7D, 0x00, 0xA0, 0x01, 0x0A, 0x8C, 0x01, 0x04, 0x9C, 0x02, 0xEF, 0xF6, 0x9D, 0x02, 0x10, 0x00, 0xFF,
        ];
        assert_eq!(pt_tag_after_fd(ns1, 0x84), Some(32000));
        assert_eq!(pt_tag_after_fd(ns1, 0x82), Some(2));
        assert_eq!(pt_tag_after_fd(ns1, 0x85), Some(0x474687));
        assert_eq!(pt_tag_after_fd(ns1, 0xA0), Some(0x0A));
        assert_eq!(asf::pt_tag(ns1, 0x84), None, "the shared reader anchors on 0x80 and misses this header");
    }

    #[test]
    fn the_street1_table_is_read_by_its_own_field_definitions() {
        // A table shaped like the NFL Street 1 disc's: 32/32/33-byte text,
        // 5-bit SNIX, 104-byte records, SNIDs out of order.
        let mut t = vec![0u8; 0x100 + 72 + 14 * 104];
        let base = 0x100;
        let defs: [(&[u8; 4], u32, u32, u32); 4] = [(b"DINS", 32, 0, 32), (b"ITNS", 256, 0, 288), (b"LANS", 256, 0, 544), (b"RANS", 264, 3, 808)];
        for (k, (tag, bits, typ, end)) in defs.iter().enumerate() {
            let d = base + 16 * k;
            t[d..d + 4].copy_from_slice(&tag[..]);
            t[d + 4..d + 8].copy_from_slice(&bits.to_be_bytes());
            t[d + 8..d + 12].copy_from_slice(&typ.to_be_bytes());
            t[d + 12..d + 16].copy_from_slice(&end.to_be_bytes());
        }
        t[base + 64..base + 68].copy_from_slice(b"XINS");
        t[base + 68..base + 72].copy_from_slice(&5u32.to_be_bytes());
        let rec0 = base + 72;
        let snids = [12u32, 2, 11, 4, 14, 13, 6, 10, 8, 5, 7, 1, 9, 3];
        for (k, snid) in snids.iter().enumerate() {
            let o = rec0 + k * 104;
            t[o..o + 4].copy_from_slice(&snid.to_be_bytes());
            let title = format!("Title {}", k + 1);
            t[o + 4..o + 4 + title.len()].copy_from_slice(title.as_bytes());
            let album = format!("Album {}", k + 1);
            t[o + 36..o + 36 + album.len()].copy_from_slice(album.as_bytes());
            let artist = "A".repeat(33); // fills the field with no terminator
            t[o + 68..o + 101].copy_from_slice(artist.as_bytes());
            t[o + 101] = ((k as u8) + 1) << 3;
        }
        let recs = read_street1_records(&t).unwrap();
        assert_eq!(recs.len(), 14);
        assert_eq!(recs[0].snid, 12);
        assert_eq!(recs[13].snid, 3);
        assert_eq!(recs[5].snix, 6);
        assert_eq!(recs[5].title, "Title 6");
        assert_eq!(recs[5].album, "Album 6");
        assert_eq!(recs[5].artist, "A".repeat(33));
        // a broken SNIX order is refused, not silently paired
        t[rec0 + 3 * 104 + 101] = 9 << 3;
        assert!(read_street1_records(&t).unwrap_err().to_string().contains("SNIX"));
    }

    fn street1_dir() -> Option<PathBuf> {
        let dir = PathBuf::from(std::env::var_os("NSR_STREET1")?);
        if dir.join("femusic.dat").is_file() {
            Some(dir)
        } else {
            eprintln!("skipped: NSR_STREET1 has no femusic.dat");
            None
        }
    }

    #[test]
    fn oracle_street1_disc_reads_fourteen_songs() {
        let Some(dir) = street1_dir() else {
            eprintln!("skipped: NSR_STREET1 unset");
            return;
        };
        let songs = read_street1(&dir).unwrap();
        assert_eq!(songs.len(), 14);
        assert_eq!(songs[0].title, "Quick To Back Down");
        assert!(songs[0].artist.contains("Bravehearts"), "{}", songs[0].artist);
        assert_eq!(songs[13].title, "Quarter");
        let mut snids: Vec<u32> = songs.iter().map(|s| s.snid).collect();
        snids.sort_unstable();
        assert_eq!(snids, (1..=14).collect::<Vec<u32>>(), "SNIDs are a permutation of 1..14");
        let (rate, channels, samples) = stream_format(&songs[0].stream);
        assert_eq!((rate, channels), (32000, 2));
        let d = decode_street1_stream(&songs[0].stream).unwrap();
        assert_eq!(d.left.len() as u64, samples, "decoded samples match PT 0x85");
        assert_eq!(d.left.len(), d.right.len());
        for s in &songs {
            let (rate, _, samples) = stream_format(&s.stream);
            let d = decode_street1_stream(&s.stream).unwrap();
            // the layout proof: block boundaries are no rougher than the
            // audio inside blocks (a wrong history order roughly doubles it)
            assert!(
                d.boundary_step_mean <= d.inner_step_mean * 1.25,
                "song {}: boundary step {:.0} vs inner {:.0}",
                s.index + 1,
                d.boundary_step_mean,
                d.inner_step_mean
            );
            assert_eq!(d.left.len() as u64, samples);
            let peak = d.left.iter().map(|v| i32::from(*v).abs()).max().unwrap_or(0);
            let rms = (d.left.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / d.left.len() as f64).sqrt();
            let secs = samples / u64::from(rate);
            eprintln!(
                "  {:>2}  snid {:>2}  {}:{:02}  {} blocks, steps {:.0}/{:.0}, peak {peak}, rms {rms:.0}  {} , {}  [{}]",
                s.index + 1, s.snid, secs / 60, secs % 60, d.blocks, d.boundary_step_mean, d.inner_step_mean, s.title, s.artist, s.album
            );
        }
    }

    #[test]
    fn oracle_street1_pack_builds_for_real() {
        let Some(dir) = street1_dir() else {
            eprintln!("skipped: NSR_STREET1 unset");
            return;
        };
        if std::env::var_os("NSR_CLEAN").is_none() {
            eprintln!("skipped: NSR_CLEAN unset");
            return;
        }
        let games = PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("NFL Street Reborn").join("Games");
        let disc = crate::soundtrack::gcm::Disc::open(&games.join("NFL Street 2 FIELDS 12-19.iso")).unwrap();
        let pristine_pathfind = disc.read_file("pathfind.dat").unwrap();
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("target").join("nfl-street-1.nsrmusic");
        let started = std::time::Instant::now();
        let report = build_pack(&dir, &pristine_pathfind, &out, &mut |k, n| eprintln!("  encoding song {} of {n}", k + 1)).unwrap();
        eprintln!("built in {:?}", started.elapsed());
        for (slot, title, artist, ms) in &report.songs {
            eprintln!("  slot {slot:>2}  {}:{:02}  {title} , {artist}", ms / 60000, (ms / 1000) % 60);
        }
        for (slot, title, why) in &report.skipped {
            eprintln!("  slot {slot:>2}  SKIPPED  {title}: {why}");
        }
        let size = std::fs::metadata(&out).unwrap().len();
        eprintln!("pack {} ({} bytes, id {})", out.display(), size, report.manifest.id);
        assert!(report.songs.len() >= 10, "only {} songs made it", report.songs.len());
        assert_eq!(report.songs.len() + report.skipped.len(), 14);
        let back = pack::read_pack(&out).unwrap();
        assert_eq!(back.manifest.songs.len(), report.songs.len());
        for (slot, _, _, _) in &report.songs {
            pack::read_blob(&back, Slot::new(*slot).unwrap()).unwrap();
        }
    }
}
