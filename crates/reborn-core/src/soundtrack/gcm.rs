//! GameCube disc image: FST read, one-file read, and a repack that
//! regenerates the flat FST with build_iso_fields.py's layout rules.
//!
//! A repack with no replacements reproduces the source byte for byte
//! (oracle 1). That property is what lets a music apply be trusted: every
//! byte the launcher did not mean to change is the byte that was there.
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::manifest::Sha256Hex;
use crate::platform::{Error, Result};

/// Historical floor: never place files earlier than old builds expect.
pub const FILES_START: u64 = 0x328000;
pub const ALIGN: u64 = 0x800;
/// A full GameCube disc.
pub const DISC_SIZE: u64 = 0x57058000;
const CHUNK: usize = 4 << 20;

pub fn align(x: u64) -> u64 {
    (x + ALIGN - 1) & !(ALIGN - 1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FstFile {
    pub name: String,
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct Disc {
    pub path: PathBuf,
    pub dol_offset: u64,
    pub dol_len: u64,
    pub fst_offset: u64,
    pub fst_size: u64,
    /// FST order, which build_iso_fields.py also uses as disc order.
    pub files: Vec<FstFile>,
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.to_path_buf(), source }
}

fn bad(path: &Path, what: &str) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_string()),
    }
}

fn be32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

impl Disc {
    pub fn open(path: &Path) -> Result<Disc> {
        let mut f = std::fs::File::open(path).map_err(|e| io(path, e))?;
        let mut hdr = [0u8; 0x440];
        f.read_exact(&mut hdr).map_err(|e| io(path, e))?;
        let dol_offset = u64::from(be32(&hdr, 0x420));
        let fst_offset = u64::from(be32(&hdr, 0x424));
        let fst_size = u64::from(be32(&hdr, 0x428));
        if dol_offset == 0 || fst_offset == 0 || fst_size < 12 {
            return Err(bad(path, "no GameCube boot header (DOL and FST offsets)"));
        }
        let mut dh = [0u8; 0x100];
        f.seek(SeekFrom::Start(dol_offset)).map_err(|e| io(path, e))?;
        f.read_exact(&mut dh).map_err(|e| io(path, e))?;
        let mut dol_len = 0u64;
        for i in 0..7 {
            dol_len = dol_len.max(u64::from(be32(&dh, i * 4)) + u64::from(be32(&dh, 0x90 + i * 4)));
        }
        for i in 0..11 {
            dol_len = dol_len.max(u64::from(be32(&dh, 0x1C + i * 4)) + u64::from(be32(&dh, 0xAC + i * 4)));
        }
        let mut fst = vec![0u8; fst_size as usize];
        f.seek(SeekFrom::Start(fst_offset)).map_err(|e| io(path, e))?;
        f.read_exact(&mut fst).map_err(|e| io(path, e))?;
        if fst[0] != 1 {
            return Err(bad(path, "no FST root entry"));
        }
        let n = be32(&fst, 8) as usize;
        if n == 0 || fst.len() < n * 12 {
            return Err(bad(path, "FST shorter than its entry count"));
        }
        let strings = &fst[n * 12..];
        let mut files = Vec::with_capacity(n - 1);
        for i in 1..n {
            let e = i * 12;
            if fst[e] != 0 {
                return Err(bad(path, "the FST has a directory entry; this packer expects a flat FST"));
            }
            let noff = (be32(&fst, e) & 0x00FF_FFFF) as usize;
            let end = strings
                .get(noff..)
                .and_then(|s| s.iter().position(|&b| b == 0))
                .ok_or_else(|| bad(path, "unterminated FST name"))?;
            let name = String::from_utf8_lossy(&strings[noff..noff + end]).into_owned();
            files.push(FstFile {
                name,
                offset: u64::from(be32(&fst, e + 4)),
                size: u64::from(be32(&fst, e + 8)),
            });
        }
        Ok(Disc { path: path.to_path_buf(), dol_offset, dol_len, fst_offset, fst_size, files })
    }

    pub fn file(&self, name: &str) -> Option<&FstFile> {
        self.files.iter().find(|f| f.name == name)
    }

    pub fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let f = self
            .file(name)
            .ok_or_else(|| bad(&self.path, &format!("the disc has no file named {name}")))?;
        let mut h = std::fs::File::open(&self.path).map_err(|e| io(&self.path, e))?;
        h.seek(SeekFrom::Start(f.offset)).map_err(|e| io(&self.path, e))?;
        let mut buf = vec![0u8; f.size as usize];
        h.read_exact(&mut buf).map_err(|e| io(&self.path, e))?;
        Ok(buf)
    }
}

/// Bytes going to the output: written, hashed, counted, in one place.
struct Sink<'a> {
    out: &'a Path,
    dst: std::io::BufWriter<std::fs::File>,
    hasher: Sha256,
    written: u64,
}

impl Sink<'_> {
    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        self.dst.write_all(bytes).map_err(|e| io(self.out, e))?;
        self.hasher.update(bytes);
        self.written += bytes.len() as u64;
        Ok(())
    }

    fn pad_to(&mut self, offset: u64) -> Result<()> {
        let zeros = [0u8; 0x1000];
        while self.written < offset {
            let n = ((offset - self.written) as usize).min(zeros.len());
            self.put(&zeros[..n])?;
        }
        Ok(())
    }
}

/// Repack to a full-size disc. See `repack_with_size`.
pub fn repack(
    disc: &Disc,
    replacements: &BTreeMap<String, Vec<u8>>,
    out: &Path,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Sha256Hex> {
    repack_with_size(disc, replacements, out, DISC_SIZE, progress)
}

/// Write `out`: verbatim head through the DOL, a regenerated FST, every file
/// in FST order (replacements substituted), zero padding to `disc_size`.
/// Returns the SHA-256 of what was written.
pub fn repack_with_size(
    disc: &Disc,
    replacements: &BTreeMap<String, Vec<u8>>,
    out: &Path,
    disc_size: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Sha256Hex> {
    for name in replacements.keys() {
        if disc.file(name).is_none() {
            return Err(bad(&disc.path, &format!("cannot replace {name}: not on the disc")));
        }
    }
    let fst_off = align(disc.dol_offset + disc.dol_len);
    let total = disc.files.len() as u32 + 1;
    let mut strtab = Vec::new();
    let mut name_off = Vec::with_capacity(disc.files.len());
    for f in &disc.files {
        name_off.push(strtab.len() as u32);
        strtab.extend_from_slice(f.name.as_bytes());
        strtab.push(0);
    }
    let fst_size = 12 * u64::from(total) + strtab.len() as u64;
    // The floor is the source disc's own first file offset: 0x328000 on the
    // shipped disc (build_iso_fields.py's historical floor), smaller on the
    // toy discs the tests build. Files never move earlier than they were.
    let floor = disc.files.iter().map(|f| f.offset).min().unwrap_or(FILES_START);
    let files_start = floor.max(align(fst_off + fst_size));
    let mut layout: Vec<(u64, u64)> = Vec::with_capacity(disc.files.len());
    let mut cur = files_start;
    for f in &disc.files {
        let size = replacements.get(&f.name).map(|d| d.len() as u64).unwrap_or(f.size);
        cur = align(cur);
        layout.push((cur, size));
        cur += size;
    }
    if cur > disc_size {
        return Err(bad(&disc.path, &format!("files overflow the disc: 0x{cur:X} > 0x{disc_size:X}")));
    }
    let mut fst = Vec::with_capacity(fst_size as usize);
    fst.push(1);
    fst.extend_from_slice(&[0, 0, 0]);
    fst.extend_from_slice(&0u32.to_be_bytes());
    fst.extend_from_slice(&total.to_be_bytes());
    for ((off, size), noff) in layout.iter().zip(&name_off) {
        fst.push(0);
        fst.extend_from_slice(&noff.to_be_bytes()[1..]);
        fst.extend_from_slice(&(*off as u32).to_be_bytes());
        fst.extend_from_slice(&(*size as u32).to_be_bytes());
    }
    fst.extend_from_slice(&strtab);
    debug_assert_eq!(fst.len() as u64, fst_size);

    let mut src = std::fs::File::open(&disc.path).map_err(|e| io(&disc.path, e))?;
    let mut sink = Sink {
        out,
        dst: std::io::BufWriter::with_capacity(CHUNK, std::fs::File::create(out).map_err(|e| io(out, e))?),
        hasher: Sha256::new(),
        written: 0,
    };
    // Head: boot.bin, bi2, apploader, DOL, verbatim, then the three FST fields.
    let head_len = (disc.dol_offset + disc.dol_len) as usize;
    let mut head = vec![0u8; head_len];
    src.read_exact(&mut head).map_err(|e| io(&disc.path, e))?;
    head[0x424..0x428].copy_from_slice(&(fst_off as u32).to_be_bytes());
    head[0x428..0x42C].copy_from_slice(&(fst_size as u32).to_be_bytes());
    head[0x42C..0x430].copy_from_slice(&(fst_size as u32).to_be_bytes());
    sink.put(&head)?;
    sink.pad_to(fst_off)?;
    sink.put(&fst)?;
    let mut buf = vec![0u8; CHUNK];
    for (f, (off, _)) in disc.files.iter().zip(&layout) {
        sink.pad_to(*off)?;
        if let Some(data) = replacements.get(&f.name) {
            sink.put(data)?;
        } else {
            src.seek(SeekFrom::Start(f.offset)).map_err(|e| io(&disc.path, e))?;
            let mut left = f.size as usize;
            while left > 0 {
                let n = left.min(CHUNK);
                src.read_exact(&mut buf[..n]).map_err(|e| io(&disc.path, e))?;
                sink.put(&buf[..n])?;
                left -= n;
            }
        }
        progress(sink.written, disc_size);
    }
    let zeros = vec![0u8; CHUNK];
    while sink.written < disc_size {
        let n = ((disc_size - sink.written) as usize).min(CHUNK);
        sink.put(&zeros[..n])?;
    }
    sink.dst.flush().map_err(|e| io(out, e))?;
    progress(sink.written, disc_size);
    let digest = hex::encode(sink.hasher.finalize());
    Ok(Sha256Hex::parse(&digest).expect("a SHA-256 hex digest"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::synthetic_disc;

    fn disc_size_of(iso: &Path) -> u64 {
        std::fs::metadata(iso).unwrap().len()
    }

    #[test]
    fn reads_the_fst_of_a_synthetic_disc() {
        let dir = tempfile::tempdir().unwrap();
        let iso = synthetic_disc(dir.path(), &[("a.dat", b"aaaa"), ("pathfind.dat", b"pppppppp"), ("z.dat", b"z")]);
        let disc = Disc::open(&iso).unwrap();
        let names: Vec<&str> = disc.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a.dat", "pathfind.dat", "z.dat"]);
        assert_eq!(disc.read_file("pathfind.dat").unwrap(), b"pppppppp");
        assert_eq!(disc.file("a.dat").unwrap().offset % ALIGN, 0);
        assert_eq!(disc.dol_len, 0x120);
        assert!(disc.read_file("nope.dat").is_err());
    }

    #[test]
    fn repack_with_no_changes_is_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let iso = synthetic_disc(dir.path(), &[("a.dat", b"aaaa"), ("pathfind.dat", b"pppppppp"), ("z.dat", b"z")]);
        let disc = Disc::open(&iso).unwrap();
        let out = dir.path().join("out.iso");
        let mut seen = 0;
        let hash = repack_with_size(&disc, &BTreeMap::new(), &out, disc_size_of(&iso), &mut |done, _| seen = done).unwrap();
        assert_eq!(std::fs::read(&iso).unwrap(), std::fs::read(&out).unwrap());
        assert_eq!(hash, crate::library::hash_file(&out).unwrap());
        assert!(seen > 0);
    }

    #[test]
    fn a_grown_file_shifts_its_followers_and_keeps_order() {
        let dir = tempfile::tempdir().unwrap();
        let iso = synthetic_disc(dir.path(), &[("a.dat", b"aaaa"), ("pathfind.dat", b"pppppppp"), ("z.dat", b"z")]);
        let disc = Disc::open(&iso).unwrap();
        let big = vec![0x42u8; 0x1801];
        let mut rep = BTreeMap::new();
        rep.insert("pathfind.dat".to_string(), big.clone());
        let out = dir.path().join("out.iso");
        repack_with_size(&disc, &rep, &out, disc_size_of(&iso) + 0x2000, &mut |_, _| {}).unwrap();
        let re = Disc::open(&out).unwrap();
        assert_eq!(re.read_file("pathfind.dat").unwrap(), big);
        assert_eq!(re.read_file("z.dat").unwrap(), b"z");
        assert_eq!(re.read_file("a.dat").unwrap(), b"aaaa");
        let p = re.file("pathfind.dat").unwrap();
        let z = re.file("z.dat").unwrap();
        assert_eq!(z.offset, align(p.offset + p.size));
        let names: Vec<String> = re.files.iter().map(|f| f.name.clone()).collect();
        assert_eq!(names, ["a.dat", "pathfind.dat", "z.dat"]);
        assert_eq!(std::fs::metadata(&out).unwrap().len(), disc_size_of(&iso) + 0x2000);
    }

    #[test]
    fn a_replacement_for_a_file_not_on_the_disc_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let iso = synthetic_disc(dir.path(), &[("a.dat", b"aaaa")]);
        let disc = Disc::open(&iso).unwrap();
        let mut rep = BTreeMap::new();
        rep.insert("ghost.dat".to_string(), vec![1]);
        let err = repack_with_size(&disc, &rep, &dir.path().join("o.iso"), 0x10000, &mut |_, _| {}).unwrap_err();
        assert!(err.to_string().contains("ghost.dat"));
    }

    /// Oracle 1: the shipped disc repacks to itself.
    #[test]
    fn oracle_repack_of_the_fields_disc_is_identical() {
        if std::env::var_os("NSR_CLEAN").is_none() {
            eprintln!("skipped: NSR_CLEAN unset");
            return;
        }
        let games = PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("NFL Street Reborn").join("Games");
        let iso = games.join("NFL Street 2 FIELDS 12-19.iso");
        let disc = Disc::open(&iso).unwrap();
        // 63 stock files plus the 8 added field containers (FIELDS 12-19).
        assert_eq!(disc.files.len(), 71);
        // The floor holds; the exact first offset follows the DOL's size.
        assert!(disc.files.iter().map(|f| f.offset).min().unwrap() >= FILES_START);
        let out = std::env::temp_dir().join("nsr-oracle-repack.iso");
        let started = std::time::Instant::now();
        let hash = repack(&disc, &BTreeMap::new(), &out, &mut |_, _| {}).unwrap();
        eprintln!("repack took {:?}", started.elapsed());
        assert_eq!(hash, crate::library::hash_file(&iso).unwrap(), "repack changed bytes");
        std::fs::remove_file(&out).unwrap();
    }
}
