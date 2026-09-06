//! Preferences a player can change whenever they like.
//!
//! Separate from [`crate::onboarding`] on purpose, and the distinction is the
//! whole reason this module exists.
//!
//! The onboarding journal is a RECORD OF WHAT HAPPENED during first run. It is
//! append-only, it latches once, and after that latch nothing is written to it
//! ever again: a journal that could still change would stop being evidence that
//! an irreversible step ran. That property is load-bearing and is pinned by a
//! test asserting the file is byte-identical after every refused write.
//!
//! But some of what first run asks is a PREFERENCE, not a historical fact.
//! Button art is the clear case: it is asked once because a new player needs an
//! answer before they play, and it has to stay changeable forever afterwards.
//! Reading it back out of the journal would have meant either re-walking a nine
//! stage wizard to change an icon set, or reopening the journal for writes and
//! losing the property that makes it evidence.
//!
//! So first run SEEDS this file and this file is the authority from then on.
//!
//! What belongs here: anything a settings screen would offer. What does not:
//! anything whose value is a claim about the past, such as which stages ran,
//! how many bytes a migration copied, or that preparation finished.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::controller::ControlsProfile;
use crate::platform::{DataDir, Platform};
use crate::{Error, Result};

/// The error every refusal here uses.
///
/// Each module keeps its own rather than sharing one, so a message can say what
/// it means in the vocabulary of the thing that refused.
fn invalid(message: String) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, message))
}

const SETTINGS_SCHEMA: &str = "nsr.settings.v1";
const SETTINGS_FILE: &str = "settings.json";

/// Where the settings file lives.
pub fn settings_path(platform: &Platform) -> Result<PathBuf> {
    Ok(platform
        .paths
        .data_root()?
        .join(DataDir::Config.relative())
        .join(SETTINGS_FILE))
}

/// The preferences, as stored.
///
/// Every field is optional and absent means "never set". That is not the same
/// as a default: the launcher can then tell the difference between a player who
/// chose Xbox and a player who has never been asked, and only the second one
/// gets asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub schema: String,
    /// Which button art to draw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls_profile: Option<ControlsProfile>,
    /// Whether ordinary updates are applied without asking. First run seeds
    /// it; this is what a launch reads, so it can be changed afterwards
    /// without re-walking the wizard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic_updates: Option<bool>,
    /// Whether the launcher may point the emulator's texture folder at the
    /// tree it composes. `None` means yes.
    ///
    /// Yes is right for a player: it is how mods reach the game. No is right
    /// for anyone who AUTHORS textures, because their folder is the source of
    /// truth and the composed tree is deleted and rebuilt on every sync.
    /// Adam, whose texture folder this launcher took over once: "this is the
    /// directory I will use for testing always."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manage_texture_folder: Option<bool>,
    /// Where the launcher looks for its release manifest. Absent means the
    /// release page for this channel; see crate::update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_source: Option<String>,
    /// Where match results are sent. Absent means the league site's ingest
    /// endpoint; see crate::league.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub league_ingest_url: Option<String>,
    /// Where content (textures, runtime, mods, game patches, the roster)
    /// comes from. Absent means the product origin; see crate::origin.
    /// Set by tests and by anyone running their own origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_origin: Option<String>,

    // The three paths a person can override, kept HERE rather than in the
    // window alone. They used to live only in the interface's own storage,
    // which the core cannot read, so a Play shortcut resolved a different
    // emulator from the one the Play button used and nothing said so. Absent
    // means "find it", which is the normal state and the shipped one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_exe: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_controller: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller_slots: Option<[Option<String>; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphics_override: Option<GraphicsOverride>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrated_texture_root: Option<PathBuf>,
}

/// Optional user-selected graphics values. Required game compatibility keys remain managed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicsOverride {
    pub label: String,
    pub resolution: u32,
    pub msaa: u32,
    pub ssaa: bool,
    #[serde(default)]
    pub legacy_gfx: Option<String>,
    #[serde(default)]
    pub legacy_reshade: Option<String>,
}

/// One of the three overridable paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathSetting {
    Runtime,
    Game,
    UserDir,
    LegacyRoot,
}

/// Record, or clear, one path a person set.
///
/// An empty string clears it, because "find it yourself" has to be
/// expressible: a box emptied in the interface must not leave the old value
/// behind on disk.
pub fn set_path(path: &Path, which: PathSetting, value: &str) -> Result<Option<PathBuf>> {
    let mut settings = Settings::load(path)?;
    let cleaned = value.trim();
    let next = if cleaned.is_empty() {
        None
    } else {
        Some(crate::session::user_path(cleaned))
    };
    match which {
        PathSetting::Runtime => settings.runtime_exe = next.clone(),
        PathSetting::Game => settings.game_path = next.clone(),
        PathSetting::UserDir => settings.user_dir = next.clone(),
        PathSetting::LegacyRoot => settings.legacy_root = next.clone(),
    }
    settings.save(path)?;
    Ok(next)
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SETTINGS_SCHEMA.to_string(),
            controls_profile: None,
            automatic_updates: None,
            manage_texture_folder: None,
            update_source: None,
            league_ingest_url: None,
            content_origin: None,
            runtime_exe: None,
            game_path: None,
            user_dir: None,
            legacy_root: None,
            primary_controller: None,
            controller_slots: None,
            graphics_override: None,
            migrated_texture_root: None,
        }
    }
}

impl Settings {
    /// Read the settings, or empty ones when there is no file yet.
    ///
    /// A missing file is the normal state before first run and is not an error.
    /// A file that cannot be parsed IS an error rather than a silent reset: a
    /// player who set something and had it quietly forgotten would have no way
    /// to know why the game looks different.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = match std::fs::read(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(Error::Io {
                    path: path.to_path_buf(),
                    source,
                })
            }
        };
        let settings: Settings = serde_json::from_slice(&raw).map_err(|e| {
            invalid(format!(
                "{} is not a readable {SETTINGS_SCHEMA} file: {e}. Nothing was \
                 changed. Delete it to start from defaults, which loses only \
                 the preferences it held.",
                path.display()
            ))
        })?;
        if settings.schema != SETTINGS_SCHEMA {
            return Err(invalid(format!(
                "{} is schema {}, and this launcher reads {SETTINGS_SCHEMA}.",
                path.display(),
                settings.schema
            )));
        }
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| invalid(format!("could not encode the settings: {e}")))?;
        std::fs::write(path, bytes).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

/// What setting the controls profile did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Set {
    /// Written.
    Changed { from: Option<ControlsProfile> },
    /// Already that value, so nothing was written.
    AlreadySet,
}

/// Set the button art, from a settings screen or from first run.
///
/// Idempotent, so a caller that does not know the current value can just set
/// the one it wants.
pub fn set_controls_profile(path: &Path, profile: ControlsProfile) -> Result<Set> {
    let mut settings = Settings::load(path)?;
    if settings.controls_profile == Some(profile) {
        return Ok(Set::AlreadySet);
    }
    let from = settings.controls_profile;
    settings.controls_profile = Some(profile);
    settings.save(path)?;
    Ok(Set::Changed { from })
}

/// The button art to draw, preferring the settings and falling back to what
/// first run recorded.
///
/// The fallback is what makes the seeding invisible: a machine that answered
/// first run before this file existed still gets the profile it chose, and the
/// first write to settings takes over from then on.
///
/// `None` means neither has an answer, which is a real state and not a reason
/// to invent one.
/// Whether to update without asking: the setting, else first run's answer,
/// else automatic.
///
/// The default when nobody has answered is `true`, and that is a decision
/// rather than an oversight: a release-to-release patch measures about a
/// megabyte, and an out of date client cannot play online. First run always
/// asks, so this only applies to an install that predates the question.
pub fn automatic_updates(settings_file: &Path, from_first_run: Option<bool>) -> Result<bool> {
    Ok(Settings::load(settings_file)?
        .automatic_updates
        .or(from_first_run)
        .unwrap_or(true))
}

/// Whether the launcher may take over the emulator's texture folder.
pub fn manage_texture_folder(path: &Path) -> Result<bool> {
    Ok(Settings::load(path)?.manage_texture_folder.unwrap_or(true))
}

/// Record it. Returns what it was.
pub fn set_manage_texture_folder(path: &Path, manage: bool) -> Result<Option<bool>> {
    let mut settings = Settings::load(path)?;
    let was = settings.manage_texture_folder;
    settings.manage_texture_folder = Some(manage);
    settings.save(path)?;
    Ok(was)
}

/// Record whether updates are automatic. Returns what it was.
pub fn set_automatic_updates(path: &Path, automatic: bool) -> Result<Option<bool>> {
    let mut settings = Settings::load(path)?;
    let was = settings.automatic_updates;
    settings.automatic_updates = Some(automatic);
    settings.save(path)?;
    Ok(was)
}

pub fn effective_controls_profile(
    settings_file: &Path,
    from_first_run: Option<ControlsProfile>,
) -> Result<Option<ControlsProfile>> {
    Ok(Settings::load(settings_file)?
        .controls_profile
        .or(from_first_run))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_reads_as_nothing_set() {
        // The normal state before first run. Absent is not an error and is not
        // a default: nothing has been chosen, and the launcher has to be able
        // to tell that from a player choosing Xbox.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        let settings = Settings::load(&path).unwrap();
        assert_eq!(settings.controls_profile, None);
        assert!(!path.exists(), "reading must not create the file");
    }

    #[test]
    fn a_profile_can_be_changed_after_first_run_has_finished() {
        // The whole point. The onboarding journal latches and stays closed, so
        // a preference kept only there could never be changed without
        // re-walking the wizard. Here it is just a value.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);

        assert_eq!(
            set_controls_profile(&path, ControlsProfile::Xbox).unwrap(),
            Set::Changed { from: None }
        );
        assert_eq!(
            set_controls_profile(&path, ControlsProfile::PlayStation).unwrap(),
            Set::Changed {
                from: Some(ControlsProfile::Xbox)
            },
            "the previous value is reported, so a caller can say what changed"
        );
        assert_eq!(
            Settings::load(&path).unwrap().controls_profile,
            Some(ControlsProfile::PlayStation)
        );
    }

    #[test]
    fn setting_the_same_profile_twice_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        set_controls_profile(&path, ControlsProfile::GameCube).unwrap();
        let before = std::fs::read(&path).unwrap();

        assert_eq!(
            set_controls_profile(&path, ControlsProfile::GameCube).unwrap(),
            Set::AlreadySet
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn first_run_seeds_it_and_settings_take_over() {
        // A machine that finished first run before this file existed still gets
        // the profile it chose, which is what makes the seeding invisible.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);

        assert_eq!(
            effective_controls_profile(&path, Some(ControlsProfile::Xbox)).unwrap(),
            Some(ControlsProfile::Xbox),
            "with nothing set, first run's answer stands"
        );

        set_controls_profile(&path, ControlsProfile::GameCube).unwrap();
        assert_eq!(
            effective_controls_profile(&path, Some(ControlsProfile::Xbox)).unwrap(),
            Some(ControlsProfile::GameCube),
            "once set, the setting wins over the historical answer"
        );

        // And neither having one is a real state.
        let empty = dir.path().join("empty.json");
        assert_eq!(effective_controls_profile(&empty, None).unwrap(), None);
    }

    #[test]
    fn an_unreadable_settings_file_is_an_error_not_a_silent_reset() {
        // A player who set something and had it quietly forgotten would have no
        // way to know why the game looks different. The message says what to do
        // and what it costs.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        std::fs::write(&path, b"{ not json").unwrap();

        let err = Settings::load(&path).unwrap_err().to_string();
        assert!(err.contains("Nothing was changed"), "{err}");
        assert!(
            err.contains("Delete it"),
            "the message must say how to recover: {err}"
        );
    }

    #[test]
    fn a_file_from_a_future_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        std::fs::write(&path, br#"{"schema":"nsr.settings.v2"}"#).unwrap();
        let err = Settings::load(&path).unwrap_err().to_string();
        assert!(err.contains("nsr.settings.v1"), "{err}");
    }

    #[test]
    fn a_path_can_be_set_cleaned_and_cleared_again() {
        // The box in the interface is a person typing, so Explorer's quotes
        // come with it, and emptying the box has to mean "find it yourself"
        // rather than leaving the old value on disk.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);

        let got = set_path(&path, PathSetting::Runtime, "  \"C:\\rt\\Dolphin.exe\"  ").unwrap();
        assert_eq!(got, Some(PathBuf::from("C:\\rt\\Dolphin.exe")));
        assert_eq!(Settings::load(&path).unwrap().runtime_exe, got);

        set_path(&path, PathSetting::Game, "C:\\g\\NSR.iso").unwrap();
        set_path(&path, PathSetting::UserDir, "C:\\u").unwrap();
        set_path(&path, PathSetting::LegacyRoot, "C:\\older-install").unwrap();
        let all = Settings::load(&path).unwrap();
        assert_eq!(all.game_path, Some(PathBuf::from("C:\\g\\NSR.iso")));
        assert_eq!(all.user_dir, Some(PathBuf::from("C:\\u")));
        assert_eq!(all.legacy_root, Some(PathBuf::from("C:\\older-install")));

        assert_eq!(set_path(&path, PathSetting::Runtime, "   ").unwrap(), None);
        let after = Settings::load(&path).unwrap();
        assert_eq!(after.runtime_exe, None, "an emptied box must clear it");
        assert_eq!(after.game_path, Some(PathBuf::from("C:\\g\\NSR.iso")), "and leave the others alone");
        set_path(&path, PathSetting::LegacyRoot, "").unwrap();
        let cleared = Settings::load(&path).unwrap();
        assert_eq!(cleared.legacy_root, None);
        assert_eq!(cleared.user_dir, Some(PathBuf::from("C:\\u")));
    }

    #[test]
    fn automatic_updates_prefers_the_setting_then_first_run_then_yes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);

        // Nobody has answered anywhere: automatic, deliberately.
        assert!(automatic_updates(&path, None).unwrap());
        // First run said no and nothing has overridden it.
        assert!(!automatic_updates(&path, Some(false)).unwrap());
        // The setting wins over first run, which is what makes it changeable.
        assert_eq!(set_automatic_updates(&path, true).unwrap(), None);
        assert!(automatic_updates(&path, Some(false)).unwrap());
        assert_eq!(set_automatic_updates(&path, false).unwrap(), Some(true));
        assert!(!automatic_updates(&path, Some(true)).unwrap());
    }

    #[test]
    fn the_texture_folder_is_managed_unless_someone_says_otherwise() {
        // On by default: that is how mods reach a player's game. Off is for
        // whoever authors the textures, whose own folder must be left alone.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        assert!(manage_texture_folder(&path).unwrap());
        assert_eq!(set_manage_texture_folder(&path, false).unwrap(), None);
        assert!(!manage_texture_folder(&path).unwrap());
        assert_eq!(set_manage_texture_folder(&path, true).unwrap(), Some(false));
        assert!(manage_texture_folder(&path).unwrap());
    }
}
