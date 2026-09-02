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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SETTINGS_SCHEMA.to_string(),
            controls_profile: None,
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
}
