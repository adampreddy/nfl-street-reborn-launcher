//! The Reborn Publisher window.
//!
//! Seven pages over `reborn_core::publish`: Status, Roster, Launcher, Game,
//! Content, History and Keys. Every operation a page can run, plan, run or
//! verify, goes through [`ops::run_operation`], which is the one path from
//! any page into the core and the one place a journal record gets written.
//! Nothing here decides a sequence, computes a rule, or parses a tool's
//! output; that all lives in `reborn_core` and in the process this window
//! starts, where it can be tested. A page reads a command and draws what
//! comes back.
//!
//! # Every command registered here, once
//!
//! This file declares every module and registers every command in one
//! `generate_handler!` call, and is not touched again after this task. That
//! is deliberate: six later tasks each build one page by editing exactly
//! two files, its own `src/<page>.rs` and `ui/<page>.js`, and none of them
//! can collide with another because none of them needs to touch this file.
//! If a page later needs a command this file does not already list, that
//! parallelism breaks for whoever needs it; the fix is to add the command
//! here deliberately; it is not something to work around inside a page.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod content;
mod game;
mod history;
mod keys;
mod launcher;
mod ops;
mod roster;
mod settings;
mod status;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            // Settings: paths, endpoints, remembered inputs. Never a secret.
            settings::publisher_settings,
            settings::publisher_set_setting,
            // The one shared operation panel's three commands.
            ops::op_status,
            ops::op_cancel,
            ops::op_clear,
            // Status, the opening page.
            status::publisher_status,
            status::publisher_environment,
            status::publisher_recent,
            // Roster push.
            roster::roster_plan,
            roster::roster_run,
            roster::roster_verify,
            // Launcher update.
            launcher::launcher_plan,
            launcher::launcher_run,
            launcher::launcher_verify,
            // Game: the Builder and the patch.
            game::game_build_plan,
            game::game_build_run,
            game::game_dump,
            game::game_cardsync,
            game::game_patch_plan,
            game::game_patch_run,
            game::game_patch_verify,
            // Content, and the runtime lint.
            content::content_plan,
            content::content_run,
            content::content_verify,
            content::content_lint_runtime,
            // History and rollback.
            history::history_list,
            history::history_versions,
            history::rollback_plan,
            history::rollback_run,
            history::rollback_verify,
            // Keys, the vault, and settings browse.
            keys::keys_state,
            keys::keys_generate,
            keys::vault_state,
            keys::vault_store,
            keys::vault_forget,
            keys::settings_browse,
        ])
        .run(tauri::generate_context!())
        .expect("the publisher window could not start");
}
