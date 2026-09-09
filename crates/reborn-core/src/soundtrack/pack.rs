//! `.nsrmusic` format 2 read and write, `.nsmp` format 1 read.
//!
//! A pack is a zip: `pack.json` plus one game-ready 40-stream blob per song.
//! Importing a pack needs no decoder and no encoder, only a splice, which is
//! what makes a shared pack instant to apply. Format 1 is what
//! `music_pack.py` wrote (magic NSMP, `slots`, `seconds`); it is read
//! unchanged so those files keep working.
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::platform::{Error, Result};
use crate::soundtrack::{asf, Refusal, Slot, SONG_SLOTS};

pub const EXT_V2: &str = "nsrmusic";
pub const EXT_V1: &str = "nsmp";
pub const MAGIC_V2: &str = "NSRMUSIC";
pub const MAGIC_V1: &str = "NSMP";
pub const FORMAT: u32 = 2;
pub const GAME: &str = "NFL Street 2 (GC)";
const MANIFEST: &str = "pack.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Song {
    pub blob: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub magic: String,
    pub format: u32,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub game: String,
    /// Keyed by slot 1..21.
    #[serde(default)]
    pub songs: BTreeMap<u8, Song>,
}

/// What music_pack.py wrote: the same zip, a different manifest shape.
#[derive(Debug, Deserialize)]
struct V1Manifest {
    magic: String,
    #[serde(default)]
    format: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    created: String,
    #[serde(default)]
    game: String,
    #[serde(default)]
    slots: BTreeMap<String, V1Slot>,
}

#[derive(Debug, Deserialize)]
struct V1Slot {
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    album: String,
    blob: String,
    #[serde(default)]
    bytes: u64,
    #[serde(default)]
    seconds: f64,
}

#[derive(Debug, Clone)]
pub struct Pack {
    pub path: PathBuf,
    pub manifest: Manifest,
}

fn refuse(path: &Path, reason: impl Into<String>) -> Error {
    Refusal::InvalidPack { path: path.to_path_buf(), reason: reason.into() }.into()
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.to_path_buf(), source }
}

fn open_archive(path: &Path) -> Result<zip::ZipArchive<std::fs::File>> {
    let file = std::fs::File::open(path).map_err(|e| io(path, e))?;
    zip::ZipArchive::new(file).map_err(|e| refuse(path, format!("not a zip file ({e})")))
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, path: &Path, name: &str) -> Result<Vec<u8>> {
    let mut entry = archive.by_name(name).map_err(|_| refuse(path, format!("no {name} inside the pack")))?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf).map_err(|e| io(path, e))?;
    Ok(buf)
}

/// Read and validate a pack's manifest. Blobs are read on demand by
/// `read_blob`; their hashes are checked there.
pub fn read_pack(path: &Path) -> Result<Pack> {
    let mut archive = open_archive(path)?;
    let raw = read_entry(&mut archive, path, MANIFEST)?;
    let probe: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|e| refuse(path, format!("pack.json is not JSON ({e})")))?;
    let magic = probe.get("magic").and_then(|m| m.as_str()).unwrap_or("");
    let manifest = match magic {
        MAGIC_V2 => serde_json::from_slice::<Manifest>(&raw)
            .map_err(|e| refuse(path, format!("pack.json is not a format 2 manifest ({e})")))?,
        MAGIC_V1 => {
            let v1: V1Manifest =
                serde_json::from_slice(&raw).map_err(|e| refuse(path, format!("pack.json is not a format 1 manifest ({e})")))?;
            convert_v1(path, v1)?
        }
        other => return Err(refuse(path, format!("magic {other:?} is not a soundtrack pack"))),
    };
    if manifest.format > FORMAT {
        return Err(refuse(path, format!("format {} is newer than this launcher reads (up to {FORMAT}); update the launcher", manifest.format)));
    }
    if manifest.songs.is_empty() && manifest.magic == MAGIC_V1 {
        return Err(refuse(path, "the pack carries no songs"));
    }
    for (slot, song) in &manifest.songs {
        if Slot::new(*slot).is_err() {
            return Err(refuse(path, format!("slot {slot} is out of range (1 to {SONG_SLOTS})")));
        }
        if song.blob.is_empty() || song.blob.contains('/') || song.blob.contains('\\') || song.blob.contains("..") {
            return Err(refuse(path, format!("slot {slot} names an unsafe blob entry {:?}", song.blob)));
        }
        if archive.by_name(&song.blob).is_err() {
            return Err(refuse(path, format!("slot {slot} names {} which is not in the pack", song.blob)));
        }
    }
    Ok(Pack { path: path.to_path_buf(), manifest })
}

fn convert_v1(path: &Path, v1: V1Manifest) -> Result<Manifest> {
    let mut songs = BTreeMap::new();
    for (key, s) in v1.slots {
        let slot: u8 = key.parse().map_err(|_| refuse(path, format!("slot key {key:?} is not a number")))?;
        songs.insert(
            slot,
            Song {
                blob: s.blob,
                sha256: None,
                bytes: s.bytes,
                duration_ms: if s.seconds > 0.0 { Some((s.seconds * 1000.0).round() as u64) } else { None },
                title: s.title,
                artist: s.artist,
                album: s.album,
            },
        );
    }
    Ok(Manifest {
        magic: v1.magic,
        format: if v1.format == 0 { 1 } else { v1.format },
        id: String::new(),
        name: v1.name,
        author: v1.author,
        description: v1.description,
        created: v1.created,
        game: v1.game,
        songs,
    })
}

/// One song's blob, validated as a game-ready 40-stream blob and against
/// the manifest hash when the pack carries one.
pub fn read_blob(pack: &Pack, slot: Slot) -> Result<Vec<u8>> {
    let song = pack
        .manifest
        .songs
        .get(&slot.get())
        .ok_or_else(|| refuse(&pack.path, format!("the pack has no song for slot {slot}")))?;
    let mut archive = open_archive(&pack.path)?;
    let blob = read_entry(&mut archive, &pack.path, &song.blob)?;
    if let Some(expected) = &song.sha256 {
        let got = hex::encode(Sha256::digest(&blob));
        if !got.eq_ignore_ascii_case(expected) {
            return Err(refuse(&pack.path, format!("slot {slot}: {} does not match its recorded hash", song.blob)));
        }
    }
    asf::validate_blob(&blob).map_err(|reason| refuse(&pack.path, format!("slot {slot}: {reason}")))?;
    Ok(blob)
}

/// A song to write into a pack.
#[derive(Debug, Clone)]
pub struct SongInput {
    pub slot: Slot,
    pub blob: Vec<u8>,
    pub title: String,
    pub artist: String,
    pub album: String,
}

fn today() -> String {
    // Days since the epoch to a civil date, no clock crate needed.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Write a format 2 pack with stored entries. Returns the manifest written.
pub fn write_pack(out: &Path, name: &str, author: &str, description: &str, songs: &[SongInput]) -> Result<Manifest> {
    if songs.is_empty() {
        return Err(refuse(out, "no songs to write"));
    }
    let mut sorted: Vec<&SongInput> = songs.iter().collect();
    sorted.sort_by_key(|s| s.slot);
    let mut id_hasher = Sha256::new();
    let mut manifest_songs = BTreeMap::new();
    for s in &sorted {
        let info = asf::validate_blob(&s.blob).map_err(|reason| refuse(out, format!("slot {}: {reason}", s.slot)))?;
        id_hasher.update(&s.blob);
        manifest_songs.insert(
            s.slot.get(),
            Song {
                blob: format!("slot{}.asf", s.slot),
                sha256: Some(hex::encode(Sha256::digest(&s.blob))),
                bytes: s.blob.len() as u64,
                duration_ms: Some(info.duration_ms),
                title: s.title.clone(),
                artist: s.artist.clone(),
                album: s.album.clone(),
            },
        );
    }
    let manifest = Manifest {
        magic: MAGIC_V2.to_string(),
        format: FORMAT,
        id: hex::encode(id_hasher.finalize())[..16].to_string(),
        name: name.to_string(),
        author: author.to_string(),
        description: description.to_string(),
        created: today(),
        game: GAME.to_string(),
        songs: manifest_songs,
    };
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
    }
    let part = out.with_extension(format!("{EXT_V2}.part"));
    let file = std::fs::File::create(&part).map_err(|e| io(&part, e))?;
    let mut z = zip::ZipWriter::new(file);
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let zerr = |e: zip::result::ZipError| Error::PlainIo(std::io::Error::other(e.to_string()));
    for s in &sorted {
        z.start_file(format!("slot{}.asf", s.slot), stored).map_err(zerr)?;
        z.write_all(&s.blob).map_err(|e| io(&part, e))?;
    }
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| Error::PlainIo(std::io::Error::other(e)))?;
    z.start_file(MANIFEST, stored).map_err(zerr)?;
    z.write_all(&json).map_err(|e| io(&part, e))?;
    z.finish().map_err(zerr)?;
    std::fs::rename(&part, out).map_err(|e| io(out, e))?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::{synthetic_stream, SYN_PT};

    fn blob() -> Vec<u8> {
        (0..40).flat_map(|_| synthetic_stream(SYN_PT, 2400, 256)).collect()
    }

    #[test]
    fn write_then_read_roundtrips_format_2() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("mine.nsrmusic");
        let man = write_pack(
            &out,
            "Mine",
            "adam",
            "test",
            &[SongInput { slot: Slot::new(3).unwrap(), blob: blob(), title: "T".into(), artist: "A".into(), album: "L".into() }],
        )
        .unwrap();
        assert_eq!(man.format, 2);
        assert_eq!(man.id.len(), 16);
        assert_eq!(man.created.len(), 10);
        let pack = read_pack(&out).unwrap();
        assert_eq!(pack.manifest.songs[&3].title, "T");
        assert_eq!(pack.manifest.songs[&3].duration_ms, Some(4000));
        assert_eq!(read_blob(&pack, Slot::new(3).unwrap()).unwrap(), blob());
        assert!(read_blob(&pack, Slot::new(4).unwrap()).is_err(), "no song there");
    }

    fn write_v1(out: &Path, json: &str, blob_name: Option<&str>) {
        let f = std::fs::File::create(out).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        if let Some(name) = blob_name {
            z.start_file(name, opts).unwrap();
            z.write_all(&blob()).unwrap();
        }
        z.start_file("pack.json", opts).unwrap();
        z.write_all(json.as_bytes()).unwrap();
        z.finish().unwrap();
    }

    #[test]
    fn reads_a_format_1_nsmp_as_music_pack_py_wrote_it() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("old.nsmp");
        write_v1(
            &out,
            r#"{"magic":"NSMP","format":1,"name":"Old","author":"x","description":"","created":"2026-07-02","game":"NFL Street 2 (GC)","slots":{"21":{"title":"Unthinkable","artist":"Alicia Keys","album":"","blob":"slot21.asf","bytes":1,"seconds":282}}}"#,
            Some("slot21.asf"),
        );
        let pack = read_pack(&out).unwrap();
        assert_eq!(pack.manifest.format, 1);
        assert_eq!(pack.manifest.songs[&21].title, "Unthinkable");
        assert_eq!(pack.manifest.songs[&21].duration_ms, Some(282_000));
        assert_eq!(read_blob(&pack, Slot::new(21).unwrap()).unwrap(), blob());
    }

    #[test]
    fn rejects_bad_magic_wrong_slot_missing_blob_and_hash_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let bad_magic = dir.path().join("a.nsrmusic");
        write_v1(&bad_magic, r#"{"magic":"NOPE","format":1,"slots":{}}"#, None);
        assert!(read_pack(&bad_magic).unwrap_err().to_string().contains("NOPE"));

        let wrong_slot = dir.path().join("b.nsmp");
        write_v1(&wrong_slot, r#"{"magic":"NSMP","format":1,"slots":{"22":{"blob":"slot22.asf"}}}"#, Some("slot22.asf"));
        assert!(read_pack(&wrong_slot).unwrap_err().to_string().contains("22"));

        let missing = dir.path().join("c.nsmp");
        write_v1(&missing, r#"{"magic":"NSMP","format":1,"slots":{"2":{"blob":"slot2.asf"}}}"#, None);
        assert!(read_pack(&missing).unwrap_err().to_string().contains("slot2.asf"));

        let newer = dir.path().join("d.nsrmusic");
        write_v1(&newer, r#"{"magic":"NSRMUSIC","format":3,"songs":{}}"#, None);
        assert!(read_pack(&newer).unwrap_err().to_string().contains("newer"));

        let out = dir.path().join("e.nsrmusic");
        write_pack(&out, "", "", "", &[SongInput { slot: Slot::new(1).unwrap(), blob: blob(), title: String::new(), artist: String::new(), album: String::new() }]).unwrap();
        // corrupt the recorded hash and expect the mismatch
        let mut pack = read_pack(&out).unwrap();
        pack.manifest.songs.get_mut(&1).unwrap().sha256 = Some("00".repeat(32));
        assert!(read_blob(&pack, Slot::new(1).unwrap()).unwrap_err().to_string().contains("hash"));
    }

    #[test]
    fn today_is_a_civil_date() {
        let d = today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
    }
}
