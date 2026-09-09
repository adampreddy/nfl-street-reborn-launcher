//! The `music` directory, shared by the roster and the soundtrack.
//!
//! The soundtrack itself lives in [`crate::soundtrack`]. What stays here is
//! the one path several subsystems agree on: `music/state.json`, the
//! [`crate::library::RebuildState`] record that tells repair a rebuilt image
//! is the user's work rather than damage. Both the roster Builder and a
//! soundtrack apply record their result there.
//!
//! Everything under `music/` is user data: see `DataDir::is_user_data`.

use std::path::{Path, PathBuf};

use crate::platform::DataDir;

/// The `music` directory itself.
///
/// Derived from [`DataDir::MusicSources`] rather than spelled out again,
/// because a second copy of a path is a second thing to keep correct.
pub fn music_root(data_root: &Path) -> PathBuf {
    let rel = DataDir::MusicSources.relative();
    let head = rel.split('/').next().unwrap_or(rel);
    data_root.join(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_music_root_is_the_first_segment_of_the_sources_dir() {
        assert_eq!(music_root(Path::new("r")), Path::new("r").join("music"));
        assert_eq!(crate::soundtrack::state::rebuild_state_path(Path::new("r")), music_root(Path::new("r")).join("state.json"));
    }
}
