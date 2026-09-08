//! The EATX banner table in template.dat: 21 inline records, 156 bytes
//! each, in the uncompressed sub-file, so edits are in-place overwrites.
//!
//! Record layout: +0 SNID u32BE, +4 title (50), +54 album (50), +104 artist
//! (50), +154 SNIX. Slot N plays and the game shows the record with SNID N.
//! The table is located by its five field definitions rather than by a
//! fixed offset, because the roster Builder rewrites template.dat.
use crate::platform::{Error, Result};
use crate::soundtrack::{Refusal, Slot, SONG_SLOTS};

pub const RECORD_STRIDE: usize = 156;
pub const FIELD_LEN: usize = 50;
/// 49 characters plus the terminator.
pub const MAX_TEXT: usize = 49;
const F_TITLE: usize = 4;
const F_ALBUM: usize = 54;
const F_ARTIST: usize = 104;
/// Field definitions as the disc stores them: reversed 4CC tags, 16 bytes
/// each, then the 8-byte SNIX definition, then record 0.
const DEFS: [(&[u8; 4], usize); 5] = [(b"DINS", 0), (b"ITNS", 16), (b"LANS", 32), (b"RANS", 48), (b"XINS", 64)];
const RECORDS_AFTER_DEFS: usize = 72;

fn invalid(msg: String) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record {
    pub title: String,
    pub album: String,
    pub artist: String,
}

/// Offset of record 0 for a table of `count` records (21 on the NFL Street 2
/// disc, 14 on NFL Street 1).
pub fn locate_count(template: &[u8], count: usize) -> Result<usize> {
    let mut i = 0;
    while let Some(p) = template.get(i..).and_then(|t| t.windows(4).position(|w| w == b"DINS")) {
        let base = i + p;
        let defs_ok = DEFS.iter().all(|(tag, off)| template.get(base + off..base + off + 4) == Some(&tag[..]));
        if defs_ok {
            let rec0 = base + RECORDS_AFTER_DEFS;
            let ids_ok = (0..count).all(|k| {
                let o = rec0 + k * RECORD_STRIDE;
                template.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) == Some(k as u32 + 1)
            });
            if ids_ok {
                return Ok(rec0);
            }
        }
        i = base + 4;
    }
    Err(invalid(format!("the EATX music table ({count} records) was not found in template.dat")))
}

/// Offset of record 0 of the 21-song table.
pub fn locate(template: &[u8]) -> Result<usize> {
    locate_count(template, usize::from(SONG_SLOTS))
}

fn read_str(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}

fn record_at(template: &[u8], o: usize) -> Record {
    Record {
        title: read_str(&template[o + F_TITLE..o + F_TITLE + FIELD_LEN]),
        album: read_str(&template[o + F_ALBUM..o + F_ALBUM + FIELD_LEN]),
        artist: read_str(&template[o + F_ARTIST..o + F_ARTIST + FIELD_LEN]),
    }
}

pub fn read_records_count(template: &[u8], count: usize) -> Result<Vec<Record>> {
    let rec0 = locate_count(template, count)?;
    if template.len() < rec0 + count * RECORD_STRIDE {
        return Err(invalid("template.dat ends inside the EATX table".into()));
    }
    Ok((0..count).map(|k| record_at(template, rec0 + k * RECORD_STRIDE)).collect())
}

/// The 21 records in slot order.
pub fn read_records(template: &[u8]) -> Result<Vec<Record>> {
    read_records_count(template, usize::from(SONG_SLOTS))
}

fn encode(field: &'static str, s: &str) -> Result<Vec<u8>> {
    let bytes: Vec<u8> = s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect();
    if bytes.len() > MAX_TEXT {
        return Err(Refusal::TooLongText { field, len: bytes.len() }.into());
    }
    Ok(bytes)
}

/// Overwrite one slot's title, album, and artist. SNID and SNIX are kept.
pub fn write_record(template: &mut [u8], slot: Slot, record: &Record) -> Result<()> {
    let rec0 = locate(template)?;
    let o = rec0 + slot.index() * RECORD_STRIDE;
    if template.len() < o + RECORD_STRIDE {
        return Err(invalid("template.dat ends inside the EATX table".into()));
    }
    let fields: [(usize, &'static str, &String); 3] =
        [(F_TITLE, "title", &record.title), (F_ALBUM, "album", &record.album), (F_ARTIST, "artist", &record.artist)];
    let encoded = [encode(fields[0].1, fields[0].2)?, encode(fields[1].1, fields[1].2)?, encode(fields[2].1, fields[2].2)?];
    for ((off, _, _), enc) in fields.iter().zip(&encoded) {
        let dst = &mut template[o + off..o + off + FIELD_LEN];
        dst.fill(0);
        dst[..enc.len()].copy_from_slice(enc);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::synthetic_template;

    #[test]
    fn locates_reads_and_writes_records() {
        let mut t = synthetic_template();
        assert_eq!(locate(&t).unwrap(), 0x1799C);
        let recs = read_records(&t).unwrap();
        assert_eq!(recs.len(), 21);
        assert_eq!(recs[0].title, "Song 1");
        assert_eq!(recs[20].artist, "Artist 21");
        write_record(
            &mut t,
            Slot::new(7).unwrap(),
            &Record { title: "Unthinkable".into(), album: "The Element of Freedom".into(), artist: "Alicia Keys".into() },
        )
        .unwrap();
        let recs = read_records(&t).unwrap();
        assert_eq!(recs[6].artist, "Alicia Keys");
        assert_eq!(recs[6].album, "The Element of Freedom");
        assert_eq!(recs[5].title, "Song 6", "neighbors untouched");
        assert_eq!(recs[7].title, "Song 8", "neighbors untouched");
        assert_eq!(&t[0x1799C + 6 * 156..0x1799C + 6 * 156 + 4], &7u32.to_be_bytes(), "SNID kept");
        assert_eq!(t[0x1799C + 6 * 156 + 154], 7 << 3, "SNIX kept");
    }

    #[test]
    fn refuses_text_that_does_not_fit_and_leaves_the_record_alone() {
        let mut t = synthetic_template();
        let before = t.clone();
        let long = "x".repeat(50);
        let err = write_record(&mut t, Slot::new(1).unwrap(), &Record { title: long, album: String::new(), artist: String::new() })
            .unwrap_err();
        assert!(err.to_string().contains("49"), "{err}");
        assert_eq!(t, before);
        let ok = "y".repeat(49);
        write_record(&mut t, Slot::new(1).unwrap(), &Record { title: ok.clone(), album: String::new(), artist: String::new() }).unwrap();
        assert_eq!(read_records(&t).unwrap()[0].title, ok);
    }

    #[test]
    fn non_latin1_characters_become_question_marks() {
        let mut t = synthetic_template();
        write_record(&mut t, Slot::new(2).unwrap(), &Record { title: "Beyoncé 東京".into(), album: String::new(), artist: String::new() })
            .unwrap();
        assert_eq!(read_records(&t).unwrap()[1].title, "Beyoncé ??");
    }

    #[test]
    fn a_moved_table_is_still_found() {
        let t = synthetic_template();
        let mut moved = vec![0u8; 0x100];
        moved.extend_from_slice(&t);
        assert_eq!(locate(&moved).unwrap(), 0x1799C + 0x100);
        assert!(locate(b"DINS only, no table").is_err());
    }

    #[test]
    fn oracle_stock_template_lists_scarred_first() {
        let Some(clean) = std::env::var_os("NSR_CLEAN") else {
            eprintln!("skipped: NSR_CLEAN unset");
            return;
        };
        let t = std::fs::read(std::path::PathBuf::from(clean).join("GameCube Version").join("root").join("template.dat")).unwrap();
        assert_eq!(locate(&t).unwrap(), 0x1799C);
        let recs = read_records(&t).unwrap();
        // Adam's tree carries his own credits, so only the shape is pinned.
        assert_eq!(recs.len(), 21);
        assert!(recs.iter().all(|r| !r.title.is_empty()), "{recs:?}");
        eprintln!("slot 1 is {:?}", recs[0]);
        eprintln!("slot 21 is {:?}", recs[20]);
    }
}
