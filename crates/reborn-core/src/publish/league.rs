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

/// The six-character game id a GameCube disc carries in its first bytes.
pub fn read_game_id(iso: &Path) -> Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(iso).map_err(|e| io_err(iso, e))?;
    let mut id = [0u8; 6];
    f.read_exact(&mut id).map_err(|e| io_err(iso, e))?;
    Ok(String::from_utf8_lossy(&id).into_owned())
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

/// Hard-link (else copy) every file under `from` into `to`, keeping relative
/// paths and dropping logs and backups. Returns (files, bytes, dropped).
fn link_tree(from: &Path, to: &Path) -> Result<(u64, u64, u64)> {
    let (mut files, mut bytes, mut dropped) = (0u64, 0u64, 0u64);
    for entry in std::fs::read_dir(from).map_err(|e| io_err(from, e))? {
        let entry = entry.map_err(|e| io_err(from, e))?;
        let src = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let dst = to.join(&name);
        if src.is_dir() {
            let (f, b, d) = link_tree(&src, &dst)?;
            files += f;
            bytes += b;
            dropped += d;
        } else if skipped(&name) {
            dropped += 1;
        } else {
            std::fs::create_dir_all(to).map_err(|e| io_err(to, e))?;
            if std::fs::hard_link(&src, &dst).is_err() {
                std::fs::copy(&src, &dst).map_err(|e| io_err(&dst, e))?;
            }
            files += 1;
            bytes += std::fs::metadata(&dst).map_err(|e| io_err(&dst, e))?.len();
        }
    }
    Ok((files, bytes, dropped))
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

/// A component is every file under its stage, so a stage inside the origin
/// would publish the origin's own files (and be cleared along with them).
fn stage_outside_origin(stage: &Path, origin: &Path) -> std::result::Result<(), String> {
    if stage.starts_with(origin) || origin.starts_with(stage) {
        return Err(format!(
            "--stage {} overlaps the origin {}. Stage somewhere outside it. Nothing was published.",
            stage.display(),
            origin.display()
        ));
    }
    Ok(())
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
/// other pin forward.
pub fn publish_league_game(req: &LeagueGameRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>> {
    if !req.iso.is_file() {
        return Ok(refuse(sink, &[format!("--iso {} is not a file. Nothing was published.", req.iso.display())]));
    }
    let id = read_game_id(req.iso)?;
    if let Err(why) = check_league_game_id(&id) {
        return Ok(refuse(sink, &[why, "Nothing was published.".to_string()]));
    }
    let tex = match resolve_textures(req.textures, &id) {
        Ok(t) => t,
        Err(why) => return Ok(refuse(sink, &[why, "Nothing was published.".to_string()])),
    };
    out!(sink, "League disc   {} (game id {id})", req.iso.display());
    out!(sink, "Textures      {}", tex.display());
    out!(sink, "Origin        {}", req.origin.display());

    if !req.apply {
        out!(sink);
        out!(sink, "Nothing was staged or written. Re-run with --apply to stage, publish league-game");
        out!(sink, "{} (members only) and re-sign the release.", req.version);
        return Ok(Outcome::Described);
    }

    let stage_dir = match req.stage {
        Some(s) => s.to_path_buf(),
        None => default_stage(req.origin, &format!("league-game-{}", req.version)),
    };
    if let Err(why) = stage_outside_origin(&stage_dir, req.origin) {
        return Ok(refuse(sink, &[why]));
    }
    if let Err(why) = fresh_stage(&stage_dir, LEAGUE_INFO_FILE) {
        return Ok(refuse(sink, &[why]));
    }
    link_file(req.iso, &stage_dir.join(LEAGUE_ISO_PATH))?;
    let (files, bytes, dropped) = link_tree(&tex, &stage_dir.join("textures").join(&id))?;
    out!(sink, "Staged        {} texture file(s), {} bytes; dropped {} log/backup file(s)", thousands(files), thousands(bytes), dropped);
    let info = serde_json::json!({
        "schema": "nsr.league-game.v1",
        "game_id": id,
        "iso": LEAGUE_ISO_PATH,
        "textures": format!("textures/{id}"),
    });
    let info_path = stage_dir.join(LEAGUE_INFO_FILE);
    std::fs::write(&info_path, serde_json::to_vec_pretty(&info).expect("static json")).map_err(|e| io_err(&info_path, e))?;

    publish_staged(
        ComponentId::LeagueGame, &stage_dir, req.origin, req.version, req.sequence,
        (req.key, req.release_version, req.release_sequence, req.notes), observer, sink,
    )
}

/// Stage `seed/league.s01` (+ `seed/seed.json`), publish `league-content`
/// members only, and re-sign the release.
pub fn publish_league_content(req: &LeagueContentRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>> {
    let is_s01 = req.seed.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("s01")).unwrap_or(false);
    if !req.seed.is_file() || !is_s01 {
        return Ok(refuse(sink, &[format!("--seed {} is not a .s01 save state. Nothing was published.", req.seed.display())]));
    }
    if !req.apply {
        out!(sink, "Seed          {}", req.seed.display());
        out!(sink, "Nothing was staged or written. Re-run with --apply.");
        return Ok(Outcome::Described);
    }
    let stage_dir = match req.stage {
        Some(s) => s.to_path_buf(),
        None => default_stage(req.origin, &format!("league-content-{}", req.version)),
    };
    if let Err(why) = stage_outside_origin(&stage_dir, req.origin) {
        return Ok(refuse(sink, &[why]));
    }
    if let Err(why) = fresh_stage(&stage_dir, LEAGUE_SEED_PATH) {
        return Ok(refuse(sink, &[why]));
    }
    link_file(req.seed, &stage_dir.join(LEAGUE_SEED_PATH))?;
    if let Some(j) = req.seed_json {
        link_file(j, &stage_dir.join(LEAGUE_SEED_JSON_PATH))?;
    }
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

    fn iso(w: &World, id: &[u8; 6]) -> PathBuf {
        let p = w.dir.path().join("league-src.iso");
        let mut bytes = id.to_vec();
        bytes.extend_from_slice(&[0u8; 2048]);
        std::fs::write(&p, bytes).unwrap();
        p
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
        let seed = w.dir.path().join("captured.s01");
        std::fs::write(&seed, b"seed state").unwrap();
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
        let seed = w.dir.path().join("notes.txt");
        std::fs::write(&seed, b"x").unwrap();
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
}
