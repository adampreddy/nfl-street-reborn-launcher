//! The GameCube profile's Gamebreaker prompt says B, because their button is B.
//!
//! # The bug this exists to fix
//!
//! The in-match Gamebreaker prompt is not static art. The HUD document's own
//! bytecode picks the glyph at runtime from a four-arm switch, one arm per
//! GAME face button. The shipped disc sets arm1 (game B) to draw X, because
//! the disc's in-match pad cave turns game B into physical X for Xbox and
//! PlayStation pads.
//!
//! [`crate::gamecube_controls`] switches that cave off for the GameCube
//! profile, so for those players game B really is physical B. Their buttons
//! are stock and correct: B activates the Gamebreaker, X switches player.
//! But the prompt still draws X, so they read "X", press X, and switch player.
//!
//! # Why it has to be done this way
//!
//! The glyph cannot be changed per profile from outside the disc:
//!
//! * It lives in the HUD document, not the DOL, so there is no code word to
//!   patch. The document's address in guest RAM moves between boots
//!   (0x81236A60 and 0x8121BFE0 in two captures of the same build), so the
//!   `[OnFrame]` list that carries the no-swap patch has nothing stable to
//!   aim at.
//! * Replacing the button *texture* is global, while the disc's B/X swap is
//!   match-gated. Swapping the art would fix the in-match prompt and break
//!   every menu, where those players are not swapped.
//!
//! So the profile gets its own copy of the one file that carries the glyph.
//! It is byte-for-byte the same size as the shipped file, so it goes into the
//! booted image in place, at the offset the disc's own file table gives, and
//! this module needs no compression code.
//!
//! # Why the offline image only
//!
//! Netplay boots its own image at `library/online` (see
//! [`crate::online_image`]). It is the same disc id, but it is *derived* --
//! pristine plus roster, nothing else -- and rebuilt whenever either of those
//! moves, so an edit made to it would be thrown away at the next rebuild.
//!
//! It also must not be edited. Netplay is lockstep, and the disc is the one
//! thing that cannot differ between two players.
//!
//! Nothing is lost by leaving it alone. The no-swap patch in
//! [`crate::gamecube_controls`] is withheld from netplay too, because Dolphin
//! does not sync `[OnFrame]` patches, so a GameCube player IS swapped there:
//! game B is their physical X, and the shipped X glyph is already right for
//! them. This only ever touches the offline image, gated on
//! `GameBuild::Offline` at the call site.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

/// The file inside the disc that carries the in-match HUD documents.
pub const FILE_NAME: &str = "uis_all.dat";

/// Both copies are exactly this long. A different length means a game update
/// changed the file and neither copy here belongs in it any more.
pub const FILE_LEN: u64 = 885_312;

/// `uis_all.dat` as the disc ships it: arm1 draws X.
pub const SHIPPED_SHA256: &str =
    "bd5c417405b07f13a00515177400afc1f24ac79c2d5678812046f0bd28058352";

/// The same file with the WHOLE glyph switch back to retail in both in-match
/// HUD documents, (3,3) and (3,16): arm0 A, arm1 B, arm2 X, arm3 Y. Built by
/// `tools/build_gamecube_glyph_variant.py` in the game tree, which refuses to
/// write anything but the full retail switch.
///
/// Every arm, not just the Gamebreaker's. The shipped switch moves two arms
/// and both moves exist only because of the X/B swap: arm1 B->X for the
/// Gamebreaker, and arm2 X->Y for the replay window, where the pad cave also
/// swaps X and Y. A GameCube-profile player has no swap at all. The first
/// variant restored arm1 only, and the replay prompt kept saying Y while X
/// did it (Adam, 2026-09-19).
pub const VARIANT_SHA256: &str =
    "ea5eed6aacd0643add70938d6ce8036f222fa85ffb793015e16e5d3047cc04d1";

/// Earlier GameCube copies a previous launcher wrote into players' images.
///
/// ⛔⛔ EVERY REPLACED VARIANT MUST STAY IN THIS LIST. A player who got one of
/// these is holding bytes that are neither the shipped copy nor today's
/// variant, and [`apply`] refuses to overwrite anything it does not recognise.
/// Drop a hash from here and every player who has it is stuck on it for good:
/// no upgrade, and no way back to the shipped copy if they leave the GameCube
/// profile, which would leave an Xbox player looking at a B prompt.
///
/// * `7cfa91be...` launchers 3.0.19 and 3.0.20: arm1 only, so the replay
///   prompt still said Y while X did it.
pub const RETIRED_VARIANT_SHA256: &[&str] =
    &["7cfa91be01ef944b7f1186a9bf0ff72631da4388cfa2a8141ebe677501adc037"];

/// Whether `have` is a copy this launcher may overwrite on its way to `want`:
/// the other current copy, or any variant an earlier launcher wrote.
fn replaceable(have: &str, other: &str) -> bool {
    have == other || RETIRED_VARIANT_SHA256.contains(&have)
}

static SHIPPED: &[u8] = include_bytes!("../assets/uis_all.shipped.dat");
static VARIANT: &[u8] = include_bytes!("../assets/uis_all.gamecube.dat");

/// What [`apply`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The image already held the copy this profile wants.
    AlreadyRight,
    /// The variant was written in: the prompt now says B.
    WroteVariant,
    /// The shipped copy was put back: the prompt says X again.
    WroteShipped,
    /// The image holds neither copy, so a game update changed the file and
    /// the copies compiled into this launcher are stale. Nothing was written.
    Unrecognised,
    /// No `uis_all.dat` in the image, or it is the wrong length.
    NotFound,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Offset and length of `uis_all.dat` in a GameCube disc image, from the FST.
fn locate(file: &mut std::fs::File) -> std::io::Result<Option<(u64, u64)>> {
    let mut buf = [0u8; 8];
    file.seek(SeekFrom::Start(0x424))?;
    file.read_exact(&mut buf)?;
    let fst_off = u32::from_be_bytes(buf[0..4].try_into().unwrap()) as u64;
    let fst_len = u32::from_be_bytes(buf[4..8].try_into().unwrap()) as usize;
    if fst_off == 0 || !(12..=(16 << 20)).contains(&fst_len) {
        return Ok(None);
    }
    let mut fst = vec![0u8; fst_len];
    file.seek(SeekFrom::Start(fst_off))?;
    file.read_exact(&mut fst)?;
    let count = u32::from_be_bytes(fst[8..12].try_into().unwrap()) as usize;
    if count == 0 || count.saturating_mul(12) > fst_len {
        return Ok(None);
    }
    let names = &fst[count * 12..];
    for i in 1..count {
        let e = &fst[i * 12..i * 12 + 12];
        if e[0] != 0 {
            continue; // a directory entry
        }
        let name_off = u32::from_be_bytes([0, e[1], e[2], e[3]]) as usize;
        let Some(rest) = names.get(name_off..) else {
            continue;
        };
        let end = rest.iter().position(|b| *b == 0).unwrap_or(0);
        if &rest[..end] != FILE_NAME.as_bytes() {
            continue;
        }
        let off = u32::from_be_bytes(e[4..8].try_into().unwrap()) as u64;
        let len = u32::from_be_bytes(e[8..12].try_into().unwrap()) as u64;
        return Ok(Some((off, len)));
    }
    Ok(None)
}

/// Make the booted image carry the copy this profile wants.
///
/// `wanted` is true for the GameCube controls profile and false for every
/// other one. Writing only ever happens when the image holds one of the two
/// known copies, so a disc a game update has moved on from is left alone and
/// reported as [`Outcome::Unrecognised`] rather than corrupted.
pub fn apply(image: &Path, wanted: bool) -> std::io::Result<Outcome> {
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(image)?;
    let Some((off, len)) = locate(&mut file)? else {
        return Ok(Outcome::NotFound);
    };
    if len != FILE_LEN {
        return Ok(Outcome::NotFound);
    }
    let mut current = vec![0u8; len as usize];
    file.seek(SeekFrom::Start(off))?;
    file.read_exact(&mut current)?;
    let have = hex(&Sha256::digest(&current));

    let (want_sha, want_bytes) = if wanted {
        (VARIANT_SHA256, VARIANT)
    } else {
        (SHIPPED_SHA256, SHIPPED)
    };
    if have == want_sha {
        return Ok(Outcome::AlreadyRight);
    }
    let other = if wanted { SHIPPED_SHA256 } else { VARIANT_SHA256 };
    if !replaceable(&have, other) {
        return Ok(Outcome::Unrecognised);
    }

    file.seek(SeekFrom::Start(off))?;
    file.write_all(want_bytes)?;
    file.flush()?;

    // Read it back: a short write into a 1.4 GB image must not pass silently.
    file.seek(SeekFrom::Start(off))?;
    let mut back = vec![0u8; len as usize];
    file.read_exact(&mut back)?;
    if hex(&Sha256::digest(&back)) != want_sha {
        return Err(std::io::Error::other(format!(
            "{FILE_NAME} did not read back as {want_sha} after writing"
        )));
    }
    Ok(if wanted { Outcome::WroteVariant } else { Outcome::WroteShipped })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two copies must stay the same length as each other and as the disc
    /// slot, or the in-place write is not possible at all.
    #[test]
    fn both_copies_are_the_slot_length() {
        assert_eq!(SHIPPED.len() as u64, FILE_LEN);
        assert_eq!(VARIANT.len() as u64, FILE_LEN);
    }

    /// The constants must describe the bytes actually compiled in, or `apply`
    /// would refuse every real disc.
    #[test]
    fn the_hashes_match_the_embedded_copies() {
        assert_eq!(hex(&Sha256::digest(SHIPPED)), SHIPPED_SHA256);
        assert_eq!(hex(&Sha256::digest(VARIANT)), VARIANT_SHA256);
    }

    /// They differ only in the glyph arms, so most of the file is shared.
    #[test]
    fn the_copies_differ_but_only_a_little() {
        assert_ne!(SHIPPED, VARIANT);
        let differing = SHIPPED.iter().zip(VARIANT).filter(|(a, b)| a != b).count();
        assert!(differing < 20_000, "unexpectedly large difference: {differing} bytes");
    }

    /// A minimal image: FST pointer at 0x424, one file entry, one name.
    fn disc_with(contents: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("NSR.iso");
        let fst_off: u32 = 0x1000;
        let data_off: u32 = 0x2000;
        let mut img = vec![0u8; data_off as usize + contents.len()];
        img[0x424..0x428].copy_from_slice(&fst_off.to_be_bytes());
        let names = b"uis_all.dat\0";
        let fst_len = 24 + names.len();
        img[0x428..0x42c].copy_from_slice(&(fst_len as u32).to_be_bytes());
        let f = fst_off as usize;
        img[f + 8..f + 12].copy_from_slice(&2u32.to_be_bytes()); // entry count
        img[f + 12] = 0; // a file, not a directory
        img[f + 13..f + 16].copy_from_slice(&[0, 0, 0]); // name at offset 0
        img[f + 16..f + 20].copy_from_slice(&data_off.to_be_bytes());
        img[f + 20..f + 24].copy_from_slice(&(contents.len() as u32).to_be_bytes());
        img[f + 24..f + 24 + names.len()].copy_from_slice(names);
        img[data_off as usize..].copy_from_slice(contents);
        std::fs::write(&path, &img).unwrap();
        (dir, path)
    }

    #[test]
    fn a_shipped_disc_gains_the_variant_and_gives_it_back() {
        let (_d, iso) = disc_with(SHIPPED);
        assert_eq!(apply(&iso, true).unwrap(), Outcome::WroteVariant);
        assert_eq!(apply(&iso, true).unwrap(), Outcome::AlreadyRight);
        assert_eq!(apply(&iso, false).unwrap(), Outcome::WroteShipped);
        assert_eq!(apply(&iso, false).unwrap(), Outcome::AlreadyRight);
        let back = std::fs::read(&iso).unwrap();
        assert_eq!(&back[0x2000..], SHIPPED);
    }

    /// An Xbox or PlayStation player, or anyone with no profile recorded, must
    /// come out of this byte-identical to the disc they downloaded. Their image
    /// already holds the shipped copy, so nothing is written at all: not a
    /// rewrite of the same bytes, no write.
    #[test]
    fn a_disc_outside_the_gamecube_profile_is_never_written() {
        let (_d, iso) = disc_with(SHIPPED);
        let before = std::fs::metadata(&iso).unwrap().modified().unwrap();
        for _ in 0..3 {
            assert_eq!(apply(&iso, false).unwrap(), Outcome::AlreadyRight);
        }
        assert_eq!(&std::fs::read(&iso).unwrap()[0x2000..], SHIPPED);
        assert_eq!(std::fs::metadata(&iso).unwrap().modified().unwrap(), before);
    }

    /// A player whose image holds a variant an earlier launcher wrote must be
    /// moved on, in both directions: forward to today's variant while they
    /// stay on the GameCube profile, and back to the shipped copy if they
    /// leave it. Refusing either strands them on the old bytes for good.
    #[test]
    fn a_retired_variant_is_replaced_both_ways() {
        for old in RETIRED_VARIANT_SHA256 {
            assert!(replaceable(old, SHIPPED_SHA256), "GameCube player stuck on {old}");
            assert!(replaceable(old, VARIANT_SHA256), "player leaving GameCube stuck on {old}");
        }
    }

    /// Recognising old variants must not widen into overwriting anything.
    #[test]
    fn the_retired_list_is_not_a_wildcard() {
        assert!(!replaceable("00".repeat(32).as_str(), SHIPPED_SHA256));
        assert!(!RETIRED_VARIANT_SHA256.contains(&SHIPPED_SHA256));
        assert!(!RETIRED_VARIANT_SHA256.contains(&VARIANT_SHA256));
    }

    /// The case that matters after a game update changes the file: leave it
    /// alone rather than write a copy that no longer belongs in this disc.
    #[test]
    fn an_unknown_copy_is_never_overwritten() {
        let mut odd = SHIPPED.to_vec();
        odd[0] ^= 0xFF;
        let (_d, iso) = disc_with(&odd);
        assert_eq!(apply(&iso, true).unwrap(), Outcome::Unrecognised);
        assert_eq!(std::fs::read(&iso).unwrap()[0x2000..], odd[..]);
    }

    #[test]
    fn a_disc_without_the_file_is_reported_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("NSR.iso");
        std::fs::write(&path, vec![0u8; 0x3000]).unwrap();
        assert_eq!(apply(&path, true).unwrap(), Outcome::NotFound);
    }
}
