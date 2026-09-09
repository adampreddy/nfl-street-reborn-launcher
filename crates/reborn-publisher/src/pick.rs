//! A general file and folder picker, for any page that needs to fill in a
//! path without asking Adam to type or paste one by hand.
//!
//! `keys::settings_browse` already opens a native dialog, but it is wired
//! to a fixed set of Settings fields and writes the chosen path into
//! settings itself; that is the right shape for a Settings field, and the
//! wrong shape for a plain input box on the Game or Roster page (a base
//! image, a roster file, a build's output path) that is not a settings
//! field at all. This module is the same underlying call --
//! `reborn-ui`'s own `pick_path` command, in
//! `crates/reborn-ui/src/main.rs`, is the precedent both this module and
//! `settings_browse` follow -- with no settings write attached: every
//! command here only ever returns the path chosen, or `None` when the
//! dialog was cancelled, which is not a refusal. A page decides what to do
//! with the answer, typically dropping it straight into the text box
//! beside the Browse button.
//!
//! Three commands, not one, because "pick a path" is not one dialog shape:
//! [`pick_file`] opens an existing file (a base image to build from, a
//! roster to publish), [`pick_save_file`] names a file that need not exist
//! yet (an output ISO, a dump target), and [`pick_folder`] has no filter to
//! choose, since a folder either is or is not the one wanted.

use std::path::PathBuf;

/// The filters this picker knows, chosen by `kind` so a page asks for the
/// shape of path it actually needs rather than getting a bare, unfiltered
/// dialog. Shared by [`pick_file`] and [`pick_save_file`], since a base
/// image and an output ISO want the same filter, only a different dialog.
fn filtered(kind: &str, dialog: rfd::FileDialog) -> Result<rfd::FileDialog, String> {
    Ok(match kind {
        "disc image" => dialog.add_filter("Game image", &["iso", "gcm", "rvz", "ciso", "wbfs"]),
        "roster" => dialog.add_filter("Reborn roster", &["rebornroster"]),
        "memory card" => dialog.add_filter("GameCube memory card", &["raw", "gcp"]),
        "executable" => dialog.add_filter("Executable", &["exe"]),
        "any" => dialog,
        other => {
            return Err(format!(
                "{other:?} is not a file kind this picker knows. Use one of disc image, roster, memory card, executable, any."
            ));
        }
    })
}

/// Open a native "choose an existing file" dialog filtered for `kind`.
/// `Ok(None)` when the dialog was cancelled: the field is left as it was,
/// which is success, not a refusal.
#[tauri::command]
pub fn pick_file(kind: String) -> Result<Option<String>, String> {
    let dialog = filtered(&kind, rfd::FileDialog::new().set_title("Reborn Publisher"))?;
    Ok(dialog.pick_file().map(|p| p.display().to_string()))
}

/// Open a native "save as" dialog filtered for `kind`, for a path that
/// names where something will be written rather than something already on
/// disk (a build's output ISO, a dump's output `.rebornroster`). Unlike
/// [`pick_file`], the chosen path is not required to exist yet.
#[tauri::command]
pub fn pick_save_file(kind: String) -> Result<Option<String>, String> {
    let dialog = filtered(&kind, rfd::FileDialog::new().set_title("Reborn Publisher"))?;
    Ok(dialog.save_file().map(|p| p.display().to_string()))
}

/// Open a native "choose a folder" dialog. No filter to choose: a folder
/// either is or is not the one wanted.
#[tauri::command]
pub fn pick_folder() -> Result<Option<String>, String> {
    let picked: Option<PathBuf> = rfd::FileDialog::new().set_title("Reborn Publisher").pick_folder();
    Ok(picked.map(|p| p.display().to_string()))
}
