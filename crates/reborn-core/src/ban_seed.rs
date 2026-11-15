//! Ranked team bans: the ranked seed with the banned teams taken out of team select.
//!
//! WHAT A BAN IS IN THE GAME (measured 2026-10-22 in the shipped runtime, both sides' team lists read back from RAM):
//! team select lists the TEAM rows whose TTYP is 0 (the 32 NFL teams) or 3 (Legends, Team Xzibit, the division
//! all-stars). Setting a row's TTYP to 2 (the draft teams' value) drops it from both players' lists; a banned team
//! that was the highlighted default just moves the cursor to its neighbour, and Randomize draws from the shortened
//! list. TVIS does NOT gate the list (the division all-stars are listed with TVIS 0).
//!
//! WHERE THE TABLE IS. The seed is a Dolphin save state, and the loaded database holds the TEAM table twice: the table
//! itself (with its field definitions) and a runtime working copy. Both are patched. Their offsets in the DECOMPRESSED
//! state are fixed for a given seed file, so they are recorded here per seed SHA-256 ([`LAYOUTS`]); a seed this file
//! does not know cannot ban, and the launcher then never offers to (the server only runs the ban step when both
//! launchers say they can).
//!
//! WHY THE TWO PCS AGREE. The derivation is a pure function of (seed bytes, sorted ban list). Only the 128 KB LZO
//! chunks that hold the banned rows are rewritten, as literal-only LZO1X blocks (no compressor, so no compressor
//! version to differ); every other chunk is copied byte for byte. The fork's loader takes each chunk's decoded length
//! as it comes (State.cpp LoadFileStateData), and a 128 KB literal block fits its 139,331-byte read buffer. Both
//! launchers hash the derived file into `/ready`'s preflight, so a disagreement is refused before Dolphin starts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The fork's State.cpp `IN_LEN` on a 64-bit build: every chunk but the last decodes to exactly this many bytes.
pub const CHUNK: usize = 128 * 1024;
/// The fork's StateHeader (gameID[6], u16, u32 size, u32, double).
const HEADER: usize = 24;
/// The loader's read buffer (State.cpp OUT_LEN). A rewritten chunk must fit it.
const LOADER_OUT_LEN: usize = CHUNK + CHUNK / 16 + 64 + 3;

/// The TEAM record and the two fields a ban touches (bit offsets from the record start, big-endian bit order).
const REC_LEN: usize = 68;
const TGID: (usize, usize) = (452, 10);
const TTYP: (usize, usize) = (517, 5);
const TTYP_NFL: u32 = 0;
const TTYP_SPECIAL: u32 = 3;
const TTYP_HIDDEN: u32 = 2;

/// The TGIDs a player may ban: the 32 NFL teams (the backend's bans.js BANNABLE).
pub fn is_bannable(tgid: u16) -> bool {
    (1..=32).contains(&tgid)
}

/// Never in a ranked game (Adam 2026-10-23): Team Xzibit (33), NFL Legends (34) and the eight division all-star
/// teams (36-43), the disc's TTYP 3 rows. Hidden from every ranked game that has the ban step, banned or not, so ranked
/// team select is the 32 NFL teams minus the bans (measured: both side lists 30 long with two bans).
pub const RANKED_HIDDEN: &[u16] = &[33, 34, 36, 37, 38, 39, 40, 41, 42, 43];

/// Every team a ranked ban seed hides: the special teams and the bans, sorted, no repeats.
pub fn hidden_for(bans: &[u16]) -> Vec<u16> {
    let mut v: Vec<u16> = normalize(bans).into_iter().chain(RANKED_HIDDEN.iter().copied()).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// One seed file whose TEAM tables have been located.
pub struct Layout {
    pub seed_sha256: &'static str,
    /// Decompressed offsets of every TEAM copy in the state.
    pub tables: &'static [usize],
    /// TGID by row, in table order (the disc's TEAM table).
    pub rows: &'static [u16],
}

/// The retail disc's TEAM table, TGID by row (template.dat, 51 rows).
const RETAIL_ROWS: &[u16] = &[
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 26, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 27, 28, 29, 32, 30, 31, 34,
    54, 33, 36, 39, 40, 41, 42, 46, 47, 48, 49, 50, 51, 52, 37, 38, 43, 53,
];

/// Every ranked seed whose tables are known. A recaptured seed needs its own entry (tools/ban_spike/ban_seed.py finds
/// the offsets), until then it simply cannot ban.
pub const LAYOUTS: &[Layout] = &[
    Layout {
        // online-content 1.0.8 ranked.s01 (captured 2026-10-13).
        seed_sha256: "61dd7e14bb6973ff6db68693750c223613393509bd63694904265bf0dddb3e3b",
        tables: &[0x474_cb45, 0x47c_e315],
        rows: RETAIL_ROWS,
    },
    Layout {
        // online-content 1.0.12 ranked.s01 (captured 2026-11-12 on the uniform-fix build; ban_seed.py found the
        // same two offsets, and the derived state was boot-tested).
        seed_sha256: "a4ce9039ec3e787edbf618b6e5b82fc579a2f1544c3a47286b97cda439ae9c94",
        tables: &[0x474_cb45, 0x47c_e315],
        rows: RETAIL_ROWS,
    },
    Layout {
        // online-content 1.0.13 ranked.s01 (captured 2026-11-14 on the GB1 Only CPU fire build + roster 2026-11-14;
        // ban_seed.py found the same two offsets).
        seed_sha256: "92f50639963d7ab82b6b43c14a3a22032d6e0f8da48ea71115f9cd9c2f7bd53e",
        tables: &[0x474_cb45, 0x47c_e315],
        rows: RETAIL_ROWS,
    },
];

pub fn layout_for(seed_sha256: &str) -> Option<&'static Layout> {
    LAYOUTS.iter().find(|l| l.seed_sha256.eq_ignore_ascii_case(seed_sha256.trim()))
}

/// The ban list as both launchers apply it: bannable only, sorted, no repeats.
pub fn normalize(bans: &[u16]) -> Vec<u16> {
    let mut v: Vec<u16> = bans.iter().copied().filter(|t| is_bannable(*t)).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// `state` with every team in `hide` removed from team select. `hide` is [`hidden_for`]'s list: sorted, no repeats,
/// non-empty, and every team in it must currently be listed (TTYP 0 or 3).
pub fn derive(state: &[u8], layout: &Layout, hide: &[u16]) -> Result<Vec<u8>, String> {
    let mut sorted = hide.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    if hide.is_empty() || sorted != hide {
        return Err("the hide list must be non-empty, sorted and without repeats".into());
    }
    let bans = hide;
    if state.len() < HEADER || u32::from_le_bytes(state[8..12].try_into().unwrap()) == 0 {
        return Err("the seed is not a compressed save state".into());
    }
    // The chunk index: (offset of its LZO block, block length).
    let mut chunks = Vec::new();
    let mut ip = HEADER;
    while ip + 4 <= state.len() {
        let n = u32::from_le_bytes(state[ip..ip + 4].try_into().unwrap()) as usize;
        if ip + 4 + n > state.len() {
            return Err("the seed's chunk list runs past the end of the file".into());
        }
        chunks.push((ip + 4, n));
        ip += 4 + n;
    }
    if ip != state.len() {
        return Err("the seed has trailing bytes after its last chunk".into());
    }

    // Decode only the chunks that hold a banned row (a row can straddle two).
    let mut plain: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    let mut edits = Vec::new();
    for &table in layout.tables {
        for &tgid in bans {
            let row = layout.rows.iter().position(|r| *r == tgid).ok_or_else(|| format!("TGID {tgid} is not on this disc"))?;
            let rec = table + row * REC_LEN;
            for k in [rec / CHUNK, (rec + REC_LEN - 1) / CHUNK] {
                if let std::collections::btree_map::Entry::Vacant(e) = plain.entry(k) {
                    let &(at, n) = chunks.get(k).ok_or("a TEAM table lies past the end of the seed")?;
                    let out = lzo1x_decompress(&state[at..at + n])?;
                    if out.len() != CHUNK {
                        return Err(format!("chunk {k} decodes to {} bytes, not {CHUNK}", out.len()));
                    }
                    e.insert(out);
                }
            }
            edits.push((rec, tgid));
        }
    }
    let mut view = ChunkView { chunks: &mut plain };
    for (rec, tgid) in edits {
        if view.bits(rec, TGID)? != u32::from(tgid) {
            return Err(format!("the TEAM row for TGID {tgid} at {rec:#x} is another team: this is not the seed the layout was made from"));
        }
        let ttyp = view.bits(rec, TTYP)?;
        if ttyp != TTYP_NFL && ttyp != TTYP_SPECIAL {
            return Err(format!("TGID {tgid} at {rec:#x} is not a listed team (TTYP {ttyp})"));
        }
        view.set_bits(rec, TTYP, TTYP_HIDDEN)?;
    }

    let mut out = Vec::with_capacity(state.len() + plain.len() * 1024);
    out.extend_from_slice(&state[..HEADER]);
    for (k, &(at, n)) in chunks.iter().enumerate() {
        match plain.get(&k) {
            Some(bytes) => {
                let block = literal_block(bytes);
                if block.len() > LOADER_OUT_LEN {
                    return Err("a rewritten chunk would not fit Dolphin's load buffer".into());
                }
                out.extend_from_slice(&(block.len() as u32).to_le_bytes());
                out.extend_from_slice(&block);
            }
            None => out.extend_from_slice(&state[at - 4..at + n]),
        }
    }
    Ok(out)
}

/// Where a derived seed lives: one file per (seed, rules, ban list), so a second game of the series reuses it. `r1` =
/// ranked rules 1 (the special teams hidden); bump it if [`RANKED_HIDDEN`] changes.
pub fn derived_path(dir: &Path, seed_sha256: &str, bans: &[u16]) -> PathBuf {
    let list: Vec<String> = bans.iter().map(u16::to_string).collect();
    let tail = if list.is_empty() { "nobans".to_string() } else { format!("ban-{}", list.join("-")) };
    dir.join(format!("ranked-{}-r1-{tail}.s01", &seed_sha256[..seed_sha256.len().min(12)]))
}

/// Derive the ranked seed for `seed` into `dir`: the special teams and `bans` (may be empty) hidden. Always derived again from the source (a stale or damaged file on disk
/// must never be what this PC boots); a file already holding exactly those bytes is kept rather than rewritten, because
/// Dolphin may have it open (the second game of a series) and Windows refuses to replace an open file. Otherwise
/// written to a temporary name, then renamed.
pub fn ensure(seed: &Path, dir: &Path, bans: &[u16]) -> Result<PathBuf, String> {
    let bans = normalize(bans);
    let state = std::fs::read(seed).map_err(|e| format!("could not read {}: {e}", seed.display()))?;
    let sha = sha256_hex(&state);
    let layout = layout_for(&sha).ok_or("this ranked seed cannot ban teams (unknown seed)")?;
    let derived = derive(&state, layout, &hidden_for(&bans))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let path = derived_path(dir, &sha, &bans);
    if std::fs::read(&path).map(|have| have == derived).unwrap_or(false) {
        return Ok(path);
    }
    let tmp = path.with_extension("s01.tmp");
    std::fs::write(&tmp, &derived).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("could not move {} into place: {e}", path.display()))?;
    Ok(path)
}

/// Can this ranked seed ban? Hashes the file (about 16 MB).
pub fn can_ban(seed: &Path) -> bool {
    std::fs::read(seed).map(|b| layout_for(&sha256_hex(&b)).is_some()).unwrap_or(false)
}

fn sha256_hex(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(b))
}

/// The decoded chunks, addressed by absolute offset in the decompressed state.
struct ChunkView<'a> {
    chunks: &'a mut BTreeMap<usize, Vec<u8>>,
}

impl ChunkView<'_> {
    fn byte(&mut self, at: usize) -> Result<&mut u8, String> {
        self.chunks.get_mut(&(at / CHUNK)).and_then(|c| c.get_mut(at % CHUNK)).ok_or_else(|| format!("byte {at:#x} was not decoded"))
    }
    fn bits(&mut self, rec: usize, (start, n): (usize, usize)) -> Result<u32, String> {
        let mut v = 0u32;
        for k in 0..n {
            let bit = start + k;
            let b = *self.byte(rec + bit / 8)?;
            v = (v << 1) | u32::from((b >> (7 - (bit % 8))) & 1);
        }
        Ok(v)
    }
    fn set_bits(&mut self, rec: usize, (start, n): (usize, usize), value: u32) -> Result<(), String> {
        for k in 0..n {
            let bit = start + k;
            let on = (value >> (n - 1 - k)) & 1 == 1;
            let mask = 1u8 << (7 - (bit % 8));
            let b = self.byte(rec + bit / 8)?;
            if on { *b |= mask } else { *b &= !mask }
        }
        Ok(())
    }
}

/// `data` as one LZO1X block of literals and the end marker: what minilzo's own final flush writes for a run with
/// no match before it.
pub fn literal_block(data: &[u8]) -> Vec<u8> {
    let t = data.len();
    let mut out = Vec::with_capacity(t + t / 255 + 8);
    if t == 0 {
    } else if t <= 238 {
        out.push((17 + t) as u8);
    } else {
        // `0` + 255-steps + remainder encodes a run of 18 + remainder (the decoder adds 15 and 3).
        let mut tt = t - 18;
        out.push(0);
        while tt > 255 {
            tt -= 255;
            out.push(0);
        }
        out.push(tt as u8);
    }
    out.extend_from_slice(data);
    out.extend_from_slice(&[0x11, 0x00, 0x00]);
    out
}

/// LZO1X decompression (minilzo's `lzo1x_decompress_safe`, as tools/lzo1x_decompress.py ports it), bounds-checked.
pub fn lzo1x_decompress(src: &[u8]) -> Result<Vec<u8>, String> {
    const BAD: &str = "the seed's compressed data is damaged";
    const M2_MAX_OFFSET: usize = 0x0800;
    let mut op: Vec<u8> = Vec::with_capacity(CHUNK);
    let mut ip = 0usize;
    let at = |i: usize| src.get(i).copied().ok_or(BAD);
    let copy_back = |op: &mut Vec<u8>, from: usize, n: usize| -> Result<(), String> {
        if from >= op.len() {
            return Err(BAD.into());
        }
        for i in 0..n {
            let b = *op.get(from + i).ok_or(BAD)?;
            op.push(b);
        }
        Ok(())
    };
    let lits = |op: &mut Vec<u8>, ip: &mut usize, n: usize| -> Result<(), String> {
        let s = src.get(*ip..*ip + n).ok_or(BAD)?;
        op.extend_from_slice(s);
        *ip += n;
        Ok(())
    };

    #[derive(PartialEq)]
    enum S { Top, Match, CopyMatch, MatchDone }
    let mut t: usize;
    let mut m_pos: usize = 0;
    let mut state;
    if at(ip)? > 17 {
        t = usize::from(at(ip)?) - 17;
        ip += 1;
        lits(&mut op, &mut ip, t)?;
        t = usize::from(at(ip)?);
        ip += 1;
        state = S::Match;
    } else {
        t = 0;
        state = S::Top;
    }
    loop {
        match state {
            S::Top => {
                t = usize::from(at(ip)?);
                ip += 1;
                if t >= 16 {
                    state = S::Match;
                    continue;
                }
                if t == 0 {
                    while at(ip)? == 0 {
                        t += 255;
                        ip += 1;
                    }
                    t += 15 + usize::from(at(ip)?);
                    ip += 1;
                }
                lits(&mut op, &mut ip, t + 3)?;
                t = usize::from(at(ip)?);
                ip += 1;
                if t >= 16 {
                    state = S::Match;
                    continue;
                }
                let back = (1 + M2_MAX_OFFSET) + (t >> 2) + (usize::from(at(ip)?) << 2);
                ip += 1;
                let from = op.len().checked_sub(back).ok_or(BAD)?;
                copy_back(&mut op, from, 3)?;
                state = S::MatchDone;
            }
            S::Match => {
                if t >= 64 {
                    let back = 1 + ((t >> 2) & 7) + (usize::from(at(ip)?) << 3);
                    ip += 1;
                    m_pos = op.len().checked_sub(back).ok_or(BAD)?;
                    t = (t >> 5) - 1;
                    state = S::CopyMatch;
                } else if t >= 32 {
                    t &= 31;
                    if t == 0 {
                        while at(ip)? == 0 {
                            t += 255;
                            ip += 1;
                        }
                        t += 31 + usize::from(at(ip)?);
                        ip += 1;
                    }
                    let off = (usize::from(at(ip)?) | (usize::from(at(ip + 1)?) << 8)) >> 2;
                    ip += 2;
                    m_pos = op.len().checked_sub(1 + off).ok_or(BAD)?;
                    state = S::CopyMatch;
                } else if t >= 16 {
                    let high = (t & 8) << 11;
                    t &= 7;
                    if t == 0 {
                        while at(ip)? == 0 {
                            t += 255;
                            ip += 1;
                        }
                        t += 7 + usize::from(at(ip)?);
                        ip += 1;
                    }
                    let off = (usize::from(at(ip)?) | (usize::from(at(ip + 1)?) << 8)) >> 2;
                    ip += 2;
                    if high + off == 0 {
                        // End of stream: every byte of the block must have been read.
                        return if ip == src.len() { Ok(op) } else { Err(BAD.into()) };
                    }
                    m_pos = op.len().checked_sub(high + off + 0x4000).ok_or(BAD)?;
                    state = S::CopyMatch;
                } else {
                    let back = 1 + (t >> 2) + (usize::from(at(ip)?) << 2);
                    ip += 1;
                    let from = op.len().checked_sub(back).ok_or(BAD)?;
                    copy_back(&mut op, from, 2)?;
                    state = S::MatchDone;
                }
            }
            S::CopyMatch => {
                copy_back(&mut op, m_pos, t + 2)?;
                state = S::MatchDone;
            }
            S::MatchDone => {
                t = usize::from(at(ip.checked_sub(2).ok_or(BAD)?)? & 3);
                if t == 0 {
                    state = S::Top;
                    continue;
                }
                lits(&mut op, &mut ip, t)?;
                t = usize::from(at(ip)?);
                ip += 1;
                state = S::Match;
            }
        }
        if op.len() > 4 * CHUNK {
            return Err(BAD.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake two-chunk state with one TEAM table straddling the chunk boundary, every chunk literal-encoded.
    fn fake_state() -> (Vec<u8>, Layout) {
        let mut plain = vec![0u8; CHUNK * 2 + 100];
        let table = CHUNK - 13 * REC_LEN - 20; // row 13 (TGID 13) straddles the boundary
        for (row, tgid) in RETAIL_ROWS.iter().enumerate() {
            let rec = table + row * REC_LEN;
            // Write TGID and TTYP straight into the flat buffer.
            let put = |buf: &mut Vec<u8>, (start, n): (usize, usize), value: u32| {
                for k in 0..n {
                    let bit = start + k;
                    let mask = 1u8 << (7 - (bit % 8));
                    if (value >> (n - 1 - k)) & 1 == 1 { buf[rec + bit / 8] |= mask } else { buf[rec + bit / 8] &= !mask }
                }
            };
            put(&mut plain, TGID, u32::from(*tgid));
            // The disc's values: NFL 0, specials 3, draft teams 2, Tutorial 17.
            put(&mut plain, TTYP, match *tgid { 1..=32 => 0, 46..=53 => 2, 54 => 17, _ => 3 });
        }
        let mut state = vec![0u8; HEADER];
        state[..6].copy_from_slice(b"GN7E69");
        state[8..12].copy_from_slice(&(plain.len() as u32).to_le_bytes());
        for c in plain.chunks(CHUNK) {
            let b = literal_block(c);
            state.extend_from_slice(&(b.len() as u32).to_le_bytes());
            state.extend_from_slice(&b);
        }
        let tables: &'static [usize] = Box::leak(vec![table].into_boxed_slice());
        (state, Layout { seed_sha256: "test", tables, rows: RETAIL_ROWS })
    }

    fn decode_all(state: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut ip = HEADER;
        while ip < state.len() {
            let n = u32::from_le_bytes(state[ip..ip + 4].try_into().unwrap()) as usize;
            out.extend(lzo1x_decompress(&state[ip + 4..ip + 4 + n]).unwrap());
            ip += 4 + n;
        }
        out
    }

    #[test]
    fn literal_blocks_round_trip_at_every_length_class() {
        for n in [1usize, 3, 4, 17, 18, 19, 238, 239, 255, 256, 273, 274, 1000, CHUNK] {
            let data: Vec<u8> = (0..n).map(|i| (i * 31 % 251) as u8).collect();
            assert_eq!(lzo1x_decompress(&literal_block(&data)).unwrap(), data, "length {n}");
        }
        assert!(literal_block(&vec![7u8; CHUNK]).len() <= LOADER_OUT_LEN);
    }

    #[test]
    fn a_ban_hides_exactly_the_banned_rows_and_nothing_else() {
        let (state, layout) = fake_state();
        let before = decode_all(&state);
        let hide = hidden_for(&[25, 13]);
        assert_eq!(hide, vec![13, 25, 33, 34, 36, 37, 38, 39, 40, 41, 42, 43]);
        let out = derive(&state, &layout, &hide).unwrap();
        let after = decode_all(&out);
        assert_eq!(before.len(), after.len());
        let diff: Vec<usize> = (0..before.len()).filter(|&i| before[i] != after[i]).collect();
        assert_eq!(diff.len(), 12, "one byte per hidden row: {diff:x?}");
        let mut plain = BTreeMap::new();
        for (k, c) in after.chunks(CHUNK).enumerate() { plain.insert(k, c.to_vec()); }
        let mut v = ChunkView { chunks: &mut plain };
        for (row, tgid) in RETAIL_ROWS.iter().enumerate() {
            let rec = layout.tables[0] + row * REC_LEN;
            let want = if hide.contains(tgid) { TTYP_HIDDEN } else { match *tgid { 1..=32 => 0, 46..=53 => 2, 54 => 17, _ => 3 } };
            assert_eq!(v.bits(rec, TTYP).unwrap(), want, "TGID {tgid}");
        }
        assert_eq!(derive(&state, &layout, &hide).unwrap(), out, "the same inputs give the same bytes");
        // No bans at all still hides the special teams: ranked rules, not a ban.
        assert_eq!(hidden_for(&[]), RANKED_HIDDEN.to_vec());
        assert!(derive(&state, &layout, &hidden_for(&[])).is_ok());
    }

    #[test]
    fn bad_lists_and_wrong_rows_are_refused() {
        let (state, layout) = fake_state();
        assert!(derive(&state, &layout, &[]).is_err());
        assert!(derive(&state, &layout, &[25, 13]).is_err(), "unsorted");
        assert!(derive(&state, &layout, &[46]).is_err(), "a draft team is not listed, so not hideable");
        assert_eq!(hidden_for(&[33, 7]), [&[7u16][..], RANKED_HIDDEN].concat(), "Xzibit is hidden by rule, never banned");
        let wrong = Layout { seed_sha256: "x", tables: &[1000], rows: RETAIL_ROWS };
        assert!(derive(&state, &wrong, &[13]).is_err(), "a layout that does not match the seed patches nothing");
        assert_eq!(normalize(&[25, 13, 13, 0, 40]), vec![13, 25]);
    }

    #[test]
    fn damaged_blocks_are_errors_not_panics() {
        for bad in [&[][..], &[0x00][..], &[0x20, 0xff, 0xff][..], &[0x11, 0x00][..], &[0x15, 1, 2][..]] {
            assert!(lzo1x_decompress(bad).is_err(), "{bad:x?}");
        }
    }

    /// The real shipped seed, when this PC has it: the Rust derivation must be byte-identical to the prototype that
    /// was boot-tested in the runtime (2026-10-23, Eagles + Ravens gone from both team lists).
    #[test]
    fn the_installed_ranked_seed_derives_to_the_boot_tested_bytes() {
        let Some(appdata) = std::env::var_os("APPDATA") else { return };
        let seed = PathBuf::from(appdata).join("NFL Street Reborn/library/online/seed/ranked.s01");
        let Ok(bytes) = std::fs::read(&seed) else { return };
        let sha = sha256_hex(&bytes);
        if sha != LAYOUTS[0].seed_sha256 {
            return; // another seed is installed: the layout test above covers the mechanics
        }
        // Eagles + Ravens banned and the special teams hidden: boot-tested 2026-10-23, both lists 30 teams, last Vikings.
        let out = derive(&bytes, layout_for(&sha).unwrap(), &hidden_for(&[13, 25])).unwrap();
        assert_eq!(sha256_hex(&out), "be9cd159d2b143479f1a183b3283ee20b6a23c5da06cce1e74ea19ffc8d65e21");
        eprintln!("the installed ranked seed derived to the boot-tested bytes");
    }

    /// Any seed named by REBORN_TEST_RANKED_SEED must be a known layout and derive; REBORN_TEST_RANKED_OUT keeps
    /// the derived state so it can be booted. Skipped when the variable is not set.
    #[test]
    fn a_named_ranked_seed_is_known_and_derives() {
        let Some(seed) = std::env::var_os("REBORN_TEST_RANKED_SEED") else { return };
        let bytes = std::fs::read(&seed).unwrap();
        let sha = sha256_hex(&bytes);
        let layout = layout_for(&sha).expect("the named seed has no layout");
        let out = derive(&bytes, layout, &hidden_for(&[13, 25])).unwrap();
        assert_ne!(out, bytes);
        if let Some(to) = std::env::var_os("REBORN_TEST_RANKED_OUT") {
            std::fs::write(to, &out).unwrap();
        }
        eprintln!("derived {} -> {}", &sha[..12], &sha256_hex(&out)[..12]);
    }
}
