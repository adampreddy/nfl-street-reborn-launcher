//! The league build and the league seed: two MEMBERS-ONLY components.
//!
//! `league-game` is the league's own disc (its own game id) plus its texture
//! tree. `league-content` is the seed save state league games boot from.
//! Both publish through the ordinary component + release path. What differs
//! is where their blobs go: `private/blobs/by-hash` in the local origin
//! (`crate::publish::stored_blob`), then the private bucket `reborn-league`
//! (`rebornctl publish upload --private`). The backend hands those out to
//! league members as short-lived signed URLs. Their manifests are public
//! and pinned like any other.
//!
//! The stage is a fresh tree per version under `<origin>-stage`, built from
//! hard links (copies across volumes), for the same reason as
//! `game_full::stage`: a component is every file under its root, so
//! publishing straight from a working folder would publish whatever else
//! sits there. Texture logs and `.blanked_off` backups are dropped while
//! staging; textures 1.0.0 once shipped `texconv.log` to every player.

use std::path::{Path, PathBuf};

use crate::inventory::{ComponentId, ComponentSpec, ScanObserver};
use crate::publish::component::{publish_component, ComponentRequest};
use crate::publish::release::{republish_pinning, CarryForward};
use crate::publish::roster::Push;
use crate::publish::{default_stage, io_err, next_sequence, out, refuse, thousands, Outcome, Sink};
use crate::Result;

pub const LEAGUE_ISO_PATH: &str = "game/league.iso";
pub const LEAGUE_INFO_FILE: &str = "league.json";
pub const LEAGUE_SEED_PATH: &str = "seed/league.s01";
pub const LEAGUE_SEED_JSON_PATH: &str = "seed/seed.json";
/// The launcher's stand-in until Adam names the league game id. A disc
/// carrying it is a test disc and must never be published.
pub const PLACEHOLDER_GAME_ID: &str = "LGXXXX";
const RETAIL_GAME_IDS: &[&str] = &["GN7E69", "FR4N31"];

#[derive(Debug)]
pub struct LeagueGameRequest<'a> {
    pub iso: &'a Path,
    /// The league texture folder: either named after the game id, or holding
    /// a folder named after it.
    pub textures: &'a Path,
    pub version: &'a str,
    /// Defaults to `<origin>-stage\league-game-<version>`.
    pub stage: Option<&'a Path>,
    pub origin: &'a Path,
    pub sequence: Option<u64>,
    pub key: Option<&'a Path>,
    pub release_version: Option<&'a str>,
    pub release_sequence: Option<u64>,
    pub notes: Option<&'a str>,
    pub apply: bool,
}

#[derive(Debug)]
pub struct LeagueContentRequest<'a> {
    /// The captured league seed (a `.s01` save state).
    pub seed: &'a Path,
    /// make_seed.py's capture record, when there is one.
    pub seed_json: Option<&'a Path>,
    pub version: &'a str,
    /// Defaults to `<origin>-stage\league-content-<version>`.
    pub stage: Option<&'a Path>,
    pub origin: &'a Path,
    pub sequence: Option<u64>,
    pub key: Option<&'a Path>,
    pub release_version: Option<&'a str>,
    pub release_sequence: Option<u64>,
    pub notes: Option<&'a str>,
    pub apply: bool,
}

/// The GameCube disc magic, big-endian at offset 0x1C of a plain image.
/// NKit v01 images keep the normal header, so they carry it too.
pub const GAMECUBE_MAGIC: [u8; 4] = [0xC2, 0x33, 0x9F, 0x3D];
const GAMECUBE_MAGIC_OFFSET: usize = 0x1C;
/// Compressed image formats Dolphin reads but the league path never ships:
/// the launcher boots `game/league.iso` as a plain disc.
const COMPRESSED_MAGICS: &[(&[u8; 4], &str)] = &[
    (b"RVZ\x01", "RVZ"),
    (b"WIA\x01", "WIA"),
    (&[0x01, 0xC0, 0x0B, 0xB1], "GCZ"),
    (b"CISO", "CISO"),
];
/// A real GameCube save state is tens of MiB; anything under 1 MiB is not
/// one (an empty file, a stub, a wrong pick). The header is not parsed.
pub const MIN_SEED_BYTES: u64 = 1 << 20;

// Test-only fault injection: fail the Nth texture link of this thread's
// next staging, so cleanup-on-error can be pinned without a real I/O fault.
#[cfg(test)]
thread_local! {
    static FAIL_LINK_AFTER: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// The six-character game id a GameCube disc carries in its first bytes.
pub fn read_game_id(iso: &Path) -> Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(iso).map_err(|e| io_err(iso, e))?;
    let mut id = [0u8; 6];
    f.read_exact(&mut id).map_err(|e| io_err(iso, e))?;
    Ok(String::from_utf8_lossy(&id).into_owned())
}

/// Check `iso` is a plain GameCube image and return its game id. The inner
/// `Err` is a refusal line; the outer one is an I/O failure opening it.
fn check_disc(iso: &Path) -> Result<std::result::Result<String, String>> {
    use std::io::Read;
    let f = std::fs::File::open(iso).map_err(|e| io_err(iso, e))?;
    let mut head = Vec::with_capacity(0x20);
    f.take(0x20).read_to_end(&mut head).map_err(|e| io_err(iso, e))?;
    if head.len() >= 4 {
        if let Some((_, name)) = COMPRESSED_MAGICS.iter().find(|(m, _)| head[..4] == m[..]) {
            return Ok(Err(format!(
                "{}: this is a compressed {name} image; convert it to a plain .iso first.",
                iso.display()
            )));
        }
    }
    if head.len() < 0x20 {
        return Ok(Err(format!(
            "{} is too short to be a GameCube disc ({} bytes; the header alone is 32).",
            iso.display(),
            head.len()
        )));
    }
    if head[GAMECUBE_MAGIC_OFFSET..GAMECUBE_MAGIC_OFFSET + 4] != GAMECUBE_MAGIC {
        return Ok(Err(format!(
            "{} is not a GameCube disc (no C2339F3D magic at offset 0x1C).",
            iso.display()
        )));
    }
    Ok(Ok(String::from_utf8_lossy(&head[..6]).into_owned()))
}

/// A league game id is six capitals or digits, and is not a retail id or
/// the launcher's placeholder.
pub fn check_league_game_id(id: &str) -> std::result::Result<(), String> {
    if id.len() != 6 || !id.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
        return Err(format!("{id:?} is not a GameCube game id (six capitals or digits)."));
    }
    if RETAIL_GAME_IDS.contains(&id) {
        return Err(format!("{id} is a RETAIL game id. The league build must carry its own id."));
    }
    if id == PLACEHOLDER_GAME_ID {
        return Err(format!("{id} is the launcher's placeholder, not a real league id. Set the real id in the disc first."));
    }
    Ok(())
}

/// The folder whose CONTENTS are the textures for `id`.
fn resolve_textures(textures: &Path, id: &str) -> std::result::Result<PathBuf, String> {
    if textures.file_name().and_then(|n| n.to_str()) == Some(id) && textures.is_dir() {
        return Ok(textures.to_path_buf());
    }
    let child = textures.join(id);
    if child.is_dir() {
        return Ok(child);
    }
    Err(format!(
        "{} is not a texture folder for {id}: name the folder {id}, or point at the folder that holds {id}.",
        textures.display()
    ))
}

/// Files a texture working folder carries that must never ship: bake logs
/// (`texconv.log` went out in textures 1.0.0), `.blanked_off` backups, and
/// Explorer's own droppings. The retail textures path has no filter of its
/// own to reuse: it publishes a folder already cleaned by hand.
fn skipped(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".log")
        || lower.contains(".blanked_off")
        || lower == "thumbs.db"
        || lower == "desktop.ini"
        || lower == ".ds_store"
}

/// A symlink, junction or any other reparse point. Never followed: a link
/// would ship whatever it points at, and a link loop would never end.
fn is_link(path: &Path, file_type: std::fs::FileType) -> std::io::Result<bool> {
    if file_type.is_symlink() {
        return Ok(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        let meta = std::fs::symlink_metadata(path)?;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(true);
        }
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(false)
}

#[derive(Debug, Default)]
struct TreeCount {
    files: u64,
    bytes: u64,
    dropped: u64,
}

/// Walk the texture tree BEFORE anything is written: count what would ship
/// (by the same `skipped` rule `link_tree` uses) and refuse any link.
fn scan_tree(dir: &Path, count: &mut TreeCount) -> Result<std::result::Result<(), String>> {
    for entry in std::fs::read_dir(dir).map_err(|e| io_err(dir, e))? {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| io_err(&path, e))?;
        if is_link(&path, ft).map_err(|e| io_err(&path, e))? {
            return Ok(Err(format!(
                "{} is a symbolic link, junction or reparse point. The league textures must be plain \
                 files and folders; replace it with the real files.",
                path.display()
            )));
        }
        if ft.is_dir() {
            if let Err(why) = scan_tree(&path, count)? {
                return Ok(Err(why));
            }
        } else if skipped(&entry.file_name().to_string_lossy()) {
            count.dropped += 1;
        } else {
            count.files += 1;
            count.bytes += entry.metadata().map_err(|e| io_err(&path, e))?.len();
        }
    }
    Ok(Ok(()))
}

/// Hard-link (else copy) every file under `from` into `to`, keeping relative
/// paths and dropping logs and backups. `scan_tree` has already refused any
/// link; directories are recognised by their own type, never by following.
fn link_tree(from: &Path, to: &Path) -> Result<()> {
    for entry in std::fs::read_dir(from).map_err(|e| io_err(from, e))? {
        let entry = entry.map_err(|e| io_err(from, e))?;
        let src = entry.path();
        let ft = entry.file_type().map_err(|e| io_err(&src, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let dst = to.join(&name);
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            link_tree(&src, &dst)?;
        } else if !skipped(&name) {
            #[cfg(test)]
            FAIL_LINK_AFTER.with(|c| -> Result<()> {
                match c.get() {
                    Some(0) => Err(io_err(&src, std::io::Error::other("injected staging failure"))),
                    Some(n) => {
                        c.set(Some(n - 1));
                        Ok(())
                    }
                    None => Ok(()),
                }
            })?;
            std::fs::create_dir_all(to).map_err(|e| io_err(to, e))?;
            if std::fs::hard_link(&src, &dst).is_err() {
                std::fs::copy(&src, &dst).map_err(|e| io_err(&dst, e))?;
            }
        }
    }
    Ok(())
}

fn link_file(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let _ = std::fs::remove_file(dst);
    if std::fs::hard_link(src, dst).is_err() {
        std::fs::copy(src, dst).map_err(|e| io_err(dst, e))?;
    }
    Ok(())
}

/// `p` made absolute, `.`/`..` folded, and its longest existing ancestor
/// canonicalized (so case, links and drive spelling match what is on disk);
/// the not-yet-existing tail is appended as written.
fn resolved(p: &Path) -> PathBuf {
    use std::path::Component as C;
    let p = if p.as_os_str().is_empty() { Path::new(".") } else { p };
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut norm = PathBuf::new();
    for c in abs.components() {
        match c {
            C::CurDir => {}
            C::ParentDir => {
                norm.pop();
            }
            other => norm.push(other),
        }
    }
    let mut tail = Vec::new();
    let mut cur = norm.clone();
    loop {
        if let Ok(mut out) = cur.canonicalize() {
            for t in tail.iter().rev() {
                out.push(t);
            }
            return out;
        }
        match (cur.file_name().map(|n| n.to_os_string()), cur.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name);
                cur = parent.to_path_buf();
            }
            _ => return norm,
        }
    }
}

/// Path components for comparison: case-folded on Windows, whose file
/// systems are case-insensitive.
fn path_key(p: &Path) -> Vec<String> {
    p.components()
        .map(|c| {
            let s = c.as_os_str().to_string_lossy().into_owned();
            if cfg!(windows) { s.to_lowercase() } else { s }
        })
        .collect()
}

/// A component is every file under its stage, and a stage is cleared before
/// it is rebuilt. So the stage must neither equal, contain nor sit inside
/// the origin or any input folder: it would publish their files, or the
/// clear would delete them.
fn check_stage_overlap(stage: &Path, guarded: &[(&str, &Path)]) -> std::result::Result<(), String> {
    let s = path_key(&resolved(stage));
    for (label, folder) in guarded {
        let g = path_key(&resolved(folder));
        if s.starts_with(&g) || g.starts_with(&s) {
            return Err(format!(
                "--stage {} overlaps the {label} {}. Stage somewhere outside it. Nothing was published.",
                stage.display(),
                folder.display()
            ));
        }
    }
    Ok(())
}

fn parent_of(p: &Path) -> &Path {
    p.parent().unwrap_or(Path::new("."))
}

/// Run `build` on a fresh stage; if it fails partway, remove the stage so a
/// rerun never meets a half-built tree.
fn staged_or_cleared(stage_dir: &Path, build: impl FnOnce() -> Result<()>) -> Result<()> {
    let result = build();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(stage_dir);
    }
    result
}

/// A stage directory is ours to clear only when it is empty or a previous
/// stage of the same component (it carries our marker file).
fn fresh_stage(dir: &Path, marker: &str) -> std::result::Result<(), String> {
    if dir.exists() {
        let empty = std::fs::read_dir(dir).map(|mut d| d.next().is_none()).unwrap_or(false);
        if !empty && !dir.join(marker).exists() {
            return Err(format!("{} exists and is not a league stage. Choose another --stage.", dir.display()));
        }
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

fn publish_staged(
    id: ComponentId,
    stage_dir: &Path,
    origin: &Path,
    version: &str,
    sequence: Option<u64>,
    carry: (Option<&Path>, Option<&str>, Option<u64>, Option<&str>),
    observer: &dyn ScanObserver,
    sink: Sink<'_>,
) -> Result<Outcome<Push>> {
    let sequence = match sequence {
        Some(s) => s,
        None => next_sequence(origin, id)?,
    };
    let spec = match ComponentSpec::new(id, version, sequence) {
        Ok(s) => s,
        Err(e) => return Ok(refuse(sink, &[e.to_string()])),
    };
    let component = match publish_component(&ComponentRequest { root: stage_dir, spec, origin, apply: true }, observer, sink)? {
        Outcome::Done(c) => c,
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Described => return Ok(Outcome::Described),
    };
    let (key, release_version, release_sequence, notes) = carry;
    let req = CarryForward { origin, id, version, key, release_version, release_sequence, notes, apply: true };
    Ok(match republish_pinning(&req, sink)? {
        Outcome::Done(s) => Outcome::Done(Push { component, release: Some(s) }),
        Outcome::Described => Outcome::Done(Push { component, release: None }),
        Outcome::Refused(r) => Outcome::Refused(r),
    })
}

/// Stage the league disc + textures + league.json, publish `league-game`
/// (blobs to the private store), and re-sign the release carrying every
/// other pin forward. Every check runs before anything is written.
pub fn publish_league_game(req: &LeagueGameRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>> {
    let nothing = || "Nothing was published.".to_string();
    if !req.iso.is_file() {
        return Ok(refuse(sink, &[format!("--iso {} is not a file. Nothing was published.", req.iso.display())]));
    }
    let id = match check_disc(req.iso)? {
        Ok(id) => id,
        Err(why) => return Ok(refuse(sink, &[why, nothing()])),
    };
    if let Err(why) = check_league_game_id(&id) {
        return Ok(refuse(sink, &[why, nothing()]));
    }
    let tex = match resolve_textures(req.textures, &id) {
        Ok(t) => t,
        Err(why) => return Ok(refuse(sink, &[why, nothing()])),
    };
    let mut count = TreeCount::default();
    if let Err(why) = scan_tree(&tex, &mut count)? {
        return Ok(refuse(sink, &[why, nothing()]));
    }
    if count.files == 0 {
        return Ok(refuse(sink, &[
            format!("{} holds no texture files to ship ({} log/backup file(s) skipped).", tex.display(), count.dropped),
            nothing(),
        ]));
    }
    let stage_dir = match req.stage {
        Some(s) => s.to_path_buf(),
        None => default_stage(req.origin, &format!("league-game-{}", req.version)),
    };
    if let Err(why) = check_stage_overlap(&stage_dir, &[
        ("origin", req.origin),
        ("disc folder", parent_of(req.iso)),
        ("texture folder", req.textures),
    ]) {
        return Ok(refuse(sink, &[why]));
    }
    out!(sink, "League disc   {} (game id {id})", req.iso.display());
    out!(sink, "Textures      {} ({} file(s), {} bytes; {} log/backup file(s) skipped)",
        tex.display(), thousands(count.files), thousands(count.bytes), count.dropped);
    out!(sink, "Stage         {}", stage_dir.display());
    out!(sink, "Origin        {}", req.origin.display());

    if !req.apply {
        out!(sink);
        out!(sink, "Nothing was staged or written. Re-run with --apply to stage, publish league-game");
        out!(sink, "{} (members only) and re-sign the release.", req.version);
        return Ok(Outcome::Described);
    }

    if let Err(why) = fresh_stage(&stage_dir, LEAGUE_INFO_FILE) {
        return Ok(refuse(sink, &[why]));
    }
    // league.json goes first: it is both a shipped file and the marker that
    // lets a rerun clear this stage, should anything below fail and the
    // cleanup itself not finish.
    staged_or_cleared(&stage_dir, || {
        let info = serde_json::json!({
            "schema": "nsr.league-game.v1",
            "game_id": id,
            "iso": LEAGUE_ISO_PATH,
            "textures": format!("textures/{id}"),
        });
        let info_path = stage_dir.join(LEAGUE_INFO_FILE);
        std::fs::write(&info_path, serde_json::to_vec_pretty(&info).expect("static json")).map_err(|e| io_err(&info_path, e))?;
        link_file(req.iso, &stage_dir.join(LEAGUE_ISO_PATH))?;
        link_tree(&tex, &stage_dir.join("textures").join(&id))
    })?;
    out!(sink, "Staged        disc, league.json and {} texture file(s)", thousands(count.files));

    publish_staged(
        ComponentId::LeagueGame, &stage_dir, req.origin, req.version, req.sequence,
        (req.key, req.release_version, req.release_sequence, req.notes), observer, sink,
    )
}

/// Stage `seed/league.s01` (+ `seed/seed.json`), publish `league-content`
/// members only, and re-sign the release. Every check runs before anything
/// is written.
pub fn publish_league_content(req: &LeagueContentRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>> {
    let is_s01 = req.seed.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("s01")).unwrap_or(false);
    if !req.seed.is_file() || !is_s01 {
        return Ok(refuse(sink, &[format!("--seed {} is not a .s01 save state. Nothing was published.", req.seed.display())]));
    }
    let len = std::fs::metadata(req.seed).map_err(|e| io_err(req.seed, e))?.len();
    if len == 0 {
        return Ok(refuse(sink, &[format!("--seed {} is empty. Nothing was published.", req.seed.display())]));
    }
    if len < MIN_SEED_BYTES {
        return Ok(refuse(sink, &[format!(
            "--seed {} is {} bytes, smaller than any real save state (at least {} bytes). Nothing was published.",
            req.seed.display(),
            thousands(len),
            thousands(MIN_SEED_BYTES)
        )]));
    }
    if let Some(j) = req.seed_json {
        if !j.is_file() {
            return Ok(refuse(sink, &[format!("--seed-json {} is not a file. Nothing was published.", j.display())]));
        }
        let bytes = std::fs::read(j).map_err(|e| io_err(j, e))?;
        if let Err(e) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            return Ok(refuse(sink, &[format!("--seed-json {} is not valid JSON ({e}). Nothing was published.", j.display())]));
        }
    }
    let stage_dir = match req.stage {
        Some(s) => s.to_path_buf(),
        None => default_stage(req.origin, &format!("league-content-{}", req.version)),
    };
    let mut guarded = vec![("origin", req.origin), ("seed folder", parent_of(req.seed))];
    if let Some(j) = req.seed_json {
        guarded.push(("seed-json folder", parent_of(j)));
    }
    if let Err(why) = check_stage_overlap(&stage_dir, &guarded) {
        return Ok(refuse(sink, &[why]));
    }
    out!(sink, "Seed          {} ({} bytes)", req.seed.display(), thousands(len));
    out!(sink, "Stage         {}", stage_dir.display());
    if !req.apply {
        out!(sink, "Nothing was staged or written. Re-run with --apply.");
        return Ok(Outcome::Described);
    }
    if let Err(why) = fresh_stage(&stage_dir, LEAGUE_SEED_PATH) {
        return Ok(refuse(sink, &[why]));
    }
    staged_or_cleared(&stage_dir, || {
        link_file(req.seed, &stage_dir.join(LEAGUE_SEED_PATH))?;
        if let Some(j) = req.seed_json {
            link_file(j, &stage_dir.join(LEAGUE_SEED_JSON_PATH))?;
        }
        Ok(())
    })?;
    publish_staged(
        ComponentId::LeagueContent, &stage_dir, req.origin, req.version, req.sequence,
        (req.key, req.release_version, req.release_sequence, req.notes), observer, sink,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::RunToCompletion;
    use crate::manifest::Component;
    use crate::publish::keys::keygen;
    use crate::publish::{manifest_path, Event};

    struct World {
        dir: tempfile::TempDir,
        origin: PathBuf,
        key: PathBuf,
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        let key = match keygen(&dir.path().join("keys"), &mut |_| {}).unwrap() {
            Outcome::Done(w) => w.publisher_path,
            o => panic!("{o:?}"),
        };
        World { dir, origin, key }
    }

    /// A plain GameCube header: the id at 0, the disc magic at 0x1C. The disc
    /// sits in its own folder so the default stage never overlaps it.
    fn iso(w: &World, id: &[u8; 6]) -> PathBuf {
        let mut bytes = vec![0u8; 2048];
        bytes[..6].copy_from_slice(id);
        bytes[0x1C..0x20].copy_from_slice(&GAMECUBE_MAGIC);
        raw_disc(w, &bytes)
    }

    fn raw_disc(w: &World, bytes: &[u8]) -> PathBuf {
        let p = w.dir.path().join("disc").join("league-src.iso");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn seed_file(w: &World, name: &str, len: usize) -> PathBuf {
        let p = w.dir.path().join("seed").join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, vec![7u8; len]).unwrap();
        p
    }

    fn content_req<'a>(w: &'a World, seed: &'a Path, seed_json: Option<&'a Path>) -> LeagueContentRequest<'a> {
        LeagueContentRequest {
            seed, seed_json, version: "1.0.0", stage: None, origin: &w.origin,
            sequence: None, key: Some(&w.key), release_version: Some("3.0.0"), release_sequence: Some(1),
            notes: None, apply: true,
        }
    }

    fn run_content(r: &LeagueContentRequest<'_>) -> (Outcome<Push>, Vec<String>) {
        let mut lines = Vec::new();
        (publish_league_content(r, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap(), lines)
    }

    fn textures(w: &World, folder: &str, files: &[&str]) -> PathBuf {
        let root = w.dir.path().join("tex").join(folder);
        for f in files {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, f.as_bytes()).unwrap();
        }
        root
    }

    fn game_req<'a>(w: &'a World, iso: &'a Path, tex: &'a Path, apply: bool) -> LeagueGameRequest<'a> {
        LeagueGameRequest {
            iso, textures: tex, version: "2026.10.08", stage: None, origin: &w.origin,
            sequence: None, key: Some(&w.key), release_version: Some("3.0.0"), release_sequence: Some(1),
            notes: Some("League build 1"), apply,
        }
    }

    fn run_game(r: &LeagueGameRequest<'_>) -> (Outcome<Push>, Vec<String>) {
        let mut lines = Vec::new();
        (publish_league_game(r, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap(), lines)
    }

    #[test]
    fn the_game_id_is_the_first_six_bytes_of_the_disc() {
        let w = world();
        assert_eq!(read_game_id(&iso(&w, b"ZZLG01")).unwrap(), "ZZLG01");
    }

    #[test]
    fn retail_ids_the_placeholder_and_garbage_are_refused() {
        for bad in ["GN7E69", "FR4N31", "LGXXXX", "zzlg01", "ZZ-G01", "ZZLG0"] {
            assert!(check_league_game_id(bad).is_err(), "{bad}");
        }
        assert!(check_league_game_id("ZZLG01").is_ok());
    }

    #[test]
    fn a_retail_disc_is_refused_before_anything_is_staged() {
        let w = world();
        let disc = iso(&w, b"GN7E69");
        let tex = textures(&w, "GN7E69", &["a.dds"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(!outcome.is_done(), "{lines:#?}");
        assert!(!w.origin.exists(), "nothing written");
    }

    #[test]
    fn a_textures_folder_for_another_game_is_refused() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "GN7E69", &["a.dds"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(!outcome.is_done());
        assert!(lines.iter().any(|l| l.contains("ZZLG01")), "{lines:#?}");
    }

    #[test]
    fn a_folder_holding_the_id_folder_is_accepted() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let parent = textures(&w, "pack", &["ZZLG01/a.dds"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &parent, true));
        assert!(outcome.is_done(), "{lines:#?}");
    }

    #[test]
    fn a_dry_run_writes_nothing() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let (outcome, _) = run_game(&game_req(&w, &disc, &tex, false));
        assert!(matches!(outcome, Outcome::Described));
        assert!(!w.origin.exists());
    }

    #[test]
    fn apply_stages_publishes_privately_skips_logs_and_signs() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds", "sub/b.png", "texconv.log", "c.dds.blanked_off"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        let Outcome::Done(push) = outcome else { panic!("{lines:#?}") };
        assert!(push.release.is_some(), "signed");

        let bytes = std::fs::read(manifest_path(&w.origin, ComponentId::LeagueGame, "2026.10.08")).unwrap();
        let c = Component::parse(&bytes).unwrap();
        assert!(c.members_only);
        assert_eq!(c.install_root, "library/league");
        let mut paths: Vec<&str> = c.files.iter().map(|f| f.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["game/league.iso", "league.json", "textures/ZZLG01/a.dds", "textures/ZZLG01/sub/b.png"]);

        let info = c.files.iter().find(|f| f.path == "league.json").unwrap();
        let info_bytes = std::fs::read(w.origin.join(crate::origin::private_blob_path(&info.sha256))).unwrap();
        let info: serde_json::Value = serde_json::from_slice(&info_bytes).unwrap();
        assert_eq!(info["game_id"], "ZZLG01");
        assert_eq!(info["schema"], "nsr.league-game.v1");
        assert!(!w.origin.join(crate::origin::BLOB_DIR).exists(), "no public blobs at all");
    }

    #[test]
    fn the_league_seed_is_staged_as_league_s01() {
        let w = world();
        let seed = seed_file(&w, "captured.s01", MIN_SEED_BYTES as usize);
        let req = LeagueContentRequest {
            seed: &seed, seed_json: None, version: "1.0.0", stage: None, origin: &w.origin,
            sequence: None, key: Some(&w.key), release_version: Some("3.0.0"), release_sequence: Some(1),
            notes: None, apply: true,
        };
        let mut lines = Vec::new();
        let outcome = publish_league_content(&req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        assert!(outcome.is_done(), "{lines:#?}");
        let c = Component::parse(&std::fs::read(manifest_path(&w.origin, ComponentId::LeagueContent, "1.0.0")).unwrap()).unwrap();
        assert!(c.members_only);
        assert_eq!(c.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["seed/league.s01"]);
    }

    #[test]
    fn a_seed_that_is_not_an_s01_is_refused() {
        let w = world();
        let seed = seed_file(&w, "notes.txt", MIN_SEED_BYTES as usize);
        let req = LeagueContentRequest {
            seed: &seed, seed_json: None, version: "1.0.0", stage: None, origin: &w.origin,
            sequence: None, key: None, release_version: None, release_sequence: None, notes: None, apply: true,
        };
        assert!(!publish_league_content(&req, &RunToCompletion, &mut |_| {}).unwrap().is_done());
    }

    fn stage_root(w: &World) -> PathBuf {
        w.dir.path().join("origin-stage")
    }

    #[test]
    fn other_id_textures_are_refused_before_anything_is_staged() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "GN7E69", &["a.dds"]);
        let (outcome, _) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(!outcome.is_done());
        assert!(!stage_root(&w).exists(), "no stage");
        assert!(!w.origin.exists(), "nothing written");
    }

    #[test]
    fn a_retail_disc_leaves_no_stage_behind() {
        let w = world();
        let disc = iso(&w, b"FR4N31");
        let tex = textures(&w, "FR4N31", &["a.dds"]);
        let (outcome, _) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(!outcome.is_done());
        assert!(!stage_root(&w).exists(), "no stage");
    }

    #[test]
    fn the_default_stage_is_a_sibling_of_the_origin() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(outcome.is_done(), "{lines:#?}");
        let staged = stage_root(&w).join("league-game-2026.10.08");
        assert!(staged.join(LEAGUE_ISO_PATH).is_file());
        assert!(staged.join(LEAGUE_INFO_FILE).is_file());
        assert!(staged.join("textures/ZZLG01/a.dds").is_file());
    }

    #[test]
    fn a_stage_inside_the_origin_is_refused() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let inside = w.origin.join("stage");
        let mut r = game_req(&w, &disc, &tex, true);
        r.stage = Some(&inside);
        let (outcome, lines) = run_game(&r);
        assert!(!outcome.is_done(), "{lines:#?}");
        assert!(!inside.exists());
    }

    #[test]
    fn a_folder_that_is_not_a_league_stage_is_never_cleared() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let mine = w.dir.path().join("my-work");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::write(mine.join("keep.txt"), b"x").unwrap();
        let mut r = game_req(&w, &disc, &tex, true);
        r.stage = Some(&mine);
        let (outcome, _) = run_game(&r);
        assert!(!outcome.is_done());
        assert!(mine.join("keep.txt").is_file());
    }

    #[test]
    fn explorer_droppings_are_not_shipped() {
        assert!(skipped("Thumbs.db") && skipped("desktop.ini") && skipped("TEXCONV.LOG") && skipped("x.png.blanked_off"));
        assert!(!skipped("a.dds") && !skipped("catalog.png"));
    }

    // ---- fix round 1 ----

    #[test]
    fn a_staging_failure_leaves_nothing_and_a_rerun_succeeds() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds", "b.dds", "c.dds"]);
        FAIL_LINK_AFTER.with(|c| c.set(Some(1)));
        let mut lines = Vec::new();
        let failed = publish_league_game(&game_req(&w, &disc, &tex, true), &RunToCompletion, &mut |e: Event| lines.push(e.line));
        FAIL_LINK_AFTER.with(|c| c.set(None));
        assert!(failed.is_err(), "{lines:#?}");
        let staged = stage_root(&w).join("league-game-2026.10.08");
        assert!(!staged.exists(), "half-built stage removed");
        assert!(!w.origin.exists());
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(outcome.is_done(), "{lines:#?}");
    }

    #[test]
    fn a_half_built_stage_with_the_marker_is_restaged() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let staged = stage_root(&w).join("league-game-2026.10.08");
        std::fs::create_dir_all(staged.join("game")).unwrap();
        std::fs::write(staged.join(LEAGUE_INFO_FILE), b"{}").unwrap();
        std::fs::write(staged.join("stale.dds"), b"old").unwrap();
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(outcome.is_done(), "{lines:#?}");
        assert!(!staged.join("stale.dds").exists());
    }

    #[test]
    fn zero_shippable_textures_are_refused_before_anything_is_written() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["texconv.log", "sub/a.dds.blanked_off", "Thumbs.db"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(!outcome.is_done());
        assert!(lines.iter().any(|l| l.contains("no texture files")), "{lines:#?}");
        assert!(!stage_root(&w).exists());
        assert!(!w.origin.exists());
    }

    fn assert_link_refused(w: &World, disc: &Path, tex: &Path, link: &Path) {
        let (outcome, lines) = run_game(&game_req(w, disc, tex, true));
        assert!(!outcome.is_done(), "{lines:#?}");
        let name = link.file_name().unwrap().to_string_lossy().into_owned();
        assert!(lines.iter().any(|l| l.contains(&name) && l.contains("link")), "{lines:#?}");
        assert!(!stage_root(w).exists());
    }

    #[test]
    fn a_symlink_in_the_textures_is_refused() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let link = tex.join("loop");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&tex, &link);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&tex, &link);
        if made.is_err() {
            eprintln!("symlinks unavailable here; skipped");
            return;
        }
        assert_link_refused(&w, &disc, &tex, &link);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_in_the_textures_is_refused() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let link = tex.join("junction");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&tex)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !made || !link.exists() {
            eprintln!("junctions unavailable here; skipped");
            return;
        }
        assert_link_refused(&w, &disc, &tex, &link);
    }

    #[test]
    fn compressed_images_are_refused_by_name() {
        let w = world();
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        for (magic, name) in [
            (b"RVZ\x01".to_vec(), "RVZ"),
            (b"WIA\x01".to_vec(), "WIA"),
            (vec![0x01, 0xC0, 0x0B, 0xB1], "GCZ"),
            (b"CISO".to_vec(), "CISO"),
        ] {
            let mut bytes = magic.clone();
            bytes.resize(4096, 0);
            let disc = raw_disc(&w, &bytes);
            let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
            assert!(!outcome.is_done(), "{name}");
            let want = format!("compressed {name} image; convert it to a plain .iso first");
            assert!(lines.iter().any(|l| l.contains(&want)), "{name}: {lines:#?}");
        }
        assert!(!stage_root(&w).exists());
    }

    #[test]
    fn a_disc_without_the_gamecube_magic_is_refused() {
        let w = world();
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let mut bytes = vec![0u8; 2048];
        bytes[..6].copy_from_slice(b"ZZLG01");
        let disc = raw_disc(&w, &bytes);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(!outcome.is_done());
        assert!(lines.iter().any(|l| l.contains("GameCube disc")), "{lines:#?}");
    }

    #[test]
    fn a_disc_shorter_than_the_header_is_a_clean_refusal() {
        let w = world();
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        for len in [0usize, 6, 0x1F] {
            let mut bytes = b"ZZLG01".to_vec();
            bytes.resize(len, 0);
            let disc = raw_disc(&w, &bytes);
            let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
            assert!(!outcome.is_done(), "{len}");
            assert!(lines.iter().any(|l| l.contains("too short")), "{len}: {lines:#?}");
        }
    }

    #[test]
    fn an_empty_or_tiny_seed_is_refused() {
        let w = world();
        for (len, want) in [(0usize, "is empty"), (MIN_SEED_BYTES as usize - 1, "smaller than")] {
            let seed = seed_file(&w, "captured.s01", len);
            let (outcome, lines) = run_content(&content_req(&w, &seed, None));
            assert!(!outcome.is_done(), "{len}");
            assert!(lines.iter().any(|l| l.contains(want)), "{len}: {lines:#?}");
        }
        assert!(!stage_root(&w).exists());
    }

    #[test]
    fn a_seed_json_must_exist_and_be_json() {
        let w = world();
        let seed = seed_file(&w, "captured.s01", MIN_SEED_BYTES as usize);
        let missing = w.dir.path().join("seed").join("missing.json");
        let (outcome, lines) = run_content(&content_req(&w, &seed, Some(&missing)));
        assert!(!outcome.is_done(), "{lines:#?}");
        let bad = w.dir.path().join("seed").join("bad.json");
        std::fs::write(&bad, b"{not json").unwrap();
        let (outcome, lines) = run_content(&content_req(&w, &seed, Some(&bad)));
        assert!(!outcome.is_done(), "{lines:#?}");
        assert!(lines.iter().any(|l| l.contains("not valid JSON")), "{lines:#?}");
        assert!(!stage_root(&w).exists());

        let good = w.dir.path().join("seed").join("seed.json");
        std::fs::write(&good, b"{\"frame\": 1}").unwrap();
        let (outcome, lines) = run_content(&content_req(&w, &seed, Some(&good)));
        assert!(outcome.is_done(), "{lines:#?}");
        let c = Component::parse(&std::fs::read(manifest_path(&w.origin, ComponentId::LeagueContent, "1.0.0")).unwrap()).unwrap();
        let mut paths: Vec<&str> = c.files.iter().map(|f| f.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec![LEAGUE_SEED_PATH, LEAGUE_SEED_JSON_PATH]);
    }

    fn assert_stage_refused(w: &World, disc: &Path, tex: &Path, stage: &Path) {
        let mut r = game_req(w, disc, tex, true);
        r.stage = Some(stage);
        let (outcome, lines) = run_game(&r);
        assert!(!outcome.is_done(), "{}: {lines:#?}", stage.display());
        assert!(lines.iter().any(|l| l.contains("overlaps")), "{}: {lines:#?}", stage.display());
        assert!(!w.origin.exists());
    }

    #[test]
    fn a_stage_overlapping_the_disc_or_texture_folder_is_refused() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let disc_dir = disc.parent().unwrap().to_path_buf();
        assert_stage_refused(&w, &disc, &tex, &disc_dir);
        assert_stage_refused(&w, &disc, &tex, &disc_dir.join("st"));
        assert_stage_refused(&w, &disc, &tex, w.dir.path());
        assert_stage_refused(&w, &disc, &tex, &tex.join("st"));
        // `..` and a not-yet-existing tail cannot slip past.
        assert_stage_refused(&w, &disc, &tex, &w.dir.path().join("nope").join("..").join("disc").join("new").join("st"));
        assert!(!disc_dir.join("st").exists());
        assert!(!tex.join("st").exists());
    }

    #[cfg(windows)]
    #[test]
    fn a_stage_overlap_is_found_whatever_the_case() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let upper = PathBuf::from(disc.parent().unwrap().to_string_lossy().to_uppercase()).join("st");
        assert_stage_refused(&w, &disc, &tex, &upper);
    }

    #[test]
    fn a_stage_overlapping_the_seed_folder_is_refused() {
        let w = world();
        let seed = seed_file(&w, "captured.s01", MIN_SEED_BYTES as usize);
        let inside = seed.parent().unwrap().join("st");
        let mut r = content_req(&w, &seed, None);
        r.stage = Some(&inside);
        let (outcome, lines) = run_content(&r);
        assert!(!outcome.is_done(), "{lines:#?}");
        assert!(lines.iter().any(|l| l.contains("overlaps")), "{lines:#?}");
        assert!(!inside.exists());
    }
}
