//! Platform-agnostic core for NFL Street Reborn Launcher.
//!
//! # The one rule
//!
//! Outside [`platform`], this crate contains no `#[cfg(windows)]`, no drive
//! letter, no `.exe` suffix, and no Win32 call. Everything that differs by
//! operating system lives behind a trait in that module.
//!
//! The plan is to build on Windows first and port afterwards. That is only
//! cheap if the seams exist from the first commit, so they do.
//!
//! # What is here so far
//!
//! - [`platform`]: the five seams, Windows implemented, macOS and Linux
//!   compiling.
//! - [`manifest`]: the signed release and component schemas.
//! - [`library`]: verify and repair, including the rule that keeps repair from
//!   deleting a user's custom soundtrack.
//! - [`runtime`]: generated emulator configuration, direct boot, and the
//!   packaging audit that keeps the debug markers out of a release.
//! - [`controller`]: device identity, one default mapping that is overridable
//!   at any time, and generated GCPadNew.ini.
//! - [`migration`]: moving a 2.1 install, saves first, mods un-applied before
//!   textures are trusted.
//!
//! - [`session`]: the launch sequence, in the core rather than in a front end,
//!   so the window and the command line cannot drift about what a launch does.
//! - [`supervisor`]: the single-instance lock, and the session record that
//!   survives the launcher going down with the game.
//!
//! - [`devices`]: finding controllers and naming them the way the emulator
//!   names them, which is the difference between a pad that works and a file
//!   that looks correct and binds nothing.
//!
//! Not written yet: SDL device enumeration, mods,
//! music, online, and the Xbox wizard.

// `deny`, not `forbid`. This crate contains exactly one `unsafe` block: the
// `ShellExecuteW` call in [`platform::imp::open_url`], which is how a link
// reaches the player's browser. `forbid` cannot be lifted for one call, and
// nothing in the standard library does that job -- handing the url to
// `explorer.exe` as an argument, which is what this used to do, opens the
// player's Documents folder instead of a browser as soon as the url has a
// query string. `deny` still turns every other `unsafe` in this crate into a
// compile error, so the guarantee the rest of the crate leans on (see
// `platform::imp`, which chooses `keyring` and `sysinfo` over Win32 for exactly
// that reason) is unchanged, and a second block would have to argue for itself
// the same way this one does.
#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod backend;
pub mod capture;
pub mod content;
pub mod controller;
pub mod controller_profiles;
pub mod controller_diagnostics;
pub mod cards;
pub mod notices;
pub mod devices;
pub mod gamepad;
pub mod game_image;
pub mod game_identity;
pub mod game_import;
pub mod game_patch;
pub mod gamecube_controls;
pub mod gamecube_glyph;
pub mod identity;
pub mod ini;
pub mod inventory;
pub mod jsgme;
pub mod layout;
pub mod library;
pub mod logging;
pub mod manifest;
pub mod migration;
pub mod mods;
pub mod music;
pub mod soundtrack;
pub mod net;
pub mod onboarding;
pub mod online_image;
pub mod origin;
#[cfg(feature = "publish")]
pub mod publish;
pub mod platform;
pub mod roster;
pub mod runtime;
pub mod relocate;
pub mod reshade;
pub mod session;
pub mod settings_law;
pub mod settings;
pub mod signing;
pub mod stats;
pub mod supervisor;
pub mod update;
pub mod user_template;
pub mod xbox;

pub use platform::{
    CredentialVault, DataDir, Error, ExitKind, FileOps, Paths, Platform, ProcessHost,
    RemovableDrive, RemovableDrives, Result, CURRENT_PLATFORM, Channel, ChannelSource};

/// The wall clock, in UTC, as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Here rather than in one module because two need it and neither owns time:
/// [`backend`] stamps every envelope with it, and [`logging`] stamps every
/// line. No dependency: civil time from seconds is Howard Hinnant's algorithm,
/// which is a dozen lines and has no calendar to get out of date.
/// The version this build was published as: `REBORN_BUILD_VERSION`, which
/// `installer/windows/build.ps1 -Version` exports for the whole build, else
/// the crate version. One place, so the update check, the user agent the
/// server logs and the version a page shows all agree.
pub fn build_version() -> &'static str {
    match option_env!("REBORN_BUILD_VERSION") {
        Some(v) if !v.trim().is_empty() => v,
        _ => env!("CARGO_PKG_VERSION"),
    }
}

pub fn utc_now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mth <= 2 { y + 1 } else { y };
    format!("{y:04}-{mth:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// The mechanical "no errors ever" check, shared by every module that produces
/// a sentence a player reads.
///
/// Task 10b established this in [`identity`]: named constants for the wording,
/// the technical detail to `tracing::warn!`, and a test that asserts the
/// wording mechanically rather than against a hand-maintained list of expected
/// strings, so a future edit cannot slip a bad message past it just by not
/// being on the list. It lives here now because four modules need exactly the
/// same rules: [`identity`], [`stats`], [`backend`] and [`session`].
///
/// The rules, and what each one is really guarding against:
///
/// - no `http`, and no `://`: a URL or a scheme word is our vocabulary, and a
///   verify link can carry the player's own sign-in code
/// - no run of three or more digits: a status code, or an operating system
///   error number
/// - no `\` and no `/`: a file path, which names a folder the player cannot
///   see and can do nothing about
/// - starts with a capital letter and ends with `.`, `?` or `!`: a sentence
///   rather than a fragment. Task 10b round 3 added this after two fragments
///   reached the test and were rewritten rather than the check being loosened
#[cfg(test)]
pub(crate) mod player_sentence {
    pub fn has_run_of_three_or_more_digits(s: &str) -> bool {
        let mut run = 0;
        for c in s.chars() {
            if c.is_ascii_digit() {
                run += 1;
                if run >= 3 {
                    return true;
                }
            } else {
                run = 0;
            }
        }
        false
    }

    pub fn assert_player_safe(msg: &str) {
        let lower = msg.to_ascii_lowercase();
        assert!(!lower.contains("http"), "message leaked transport jargon: {msg}");
        assert!(!msg.contains("://"), "message leaked a URL: {msg}");
        assert!(
            !has_run_of_three_or_more_digits(msg),
            "message leaked a status code or an error number: {msg}"
        );
        assert!(!msg.contains('\\'), "message leaked a file path: {msg}");
        assert!(!msg.contains('/'), "message leaked a file path: {msg}");
        let first = msg.chars().next().expect("a non-empty message");
        assert!(
            first.is_ascii_uppercase(),
            "message is a fragment, not a sentence (no capital letter to start): {msg}"
        );
        let last = msg.chars().last().expect("a non-empty message");
        assert!(
            matches!(last, '.' | '?' | '!'),
            "message is a fragment, not a sentence (no closing punctuation): {msg}"
        );
    }
}

/// Reading this workspace's own source, for the rules a type cannot express.
///
/// Several rules on this branch are about what a call site is allowed to LOOK
/// like, not about what any value is at run time: no logging call may
/// interpolate a credential, and no player-facing refusal may be built out of
/// a `format!`. Neither can be observed by running code, because the defect is
/// a line somebody has not written yet. So the tests read the source.
///
/// Two source-scanning tests on this branch have already passed by matching
/// their own text, and a third truncated the text it searched at the first
/// `#[cfg(test)]` and so never looked at production code that came after an
/// inline test module. Both are properties of the reading, not of the rule, so
/// the reading lives here once and is shared:
///
/// - [`code_only`] blanks every comment, string literal and character literal,
///   so a bracket, a keyword or a needle inside a comment or a message is
///   never mistaken for code
/// - [`production_only`] blanks EVERY `#[cfg(test)]` module, not the text from
///   the first one onwards, so a test's own words are excluded without
///   excluding the production code that follows it
/// - [`closing_bracket`] counts on the blanked view, so an unbalanced bracket
///   inside a message cannot truncate what is scanned
#[cfg(test)]
pub(crate) mod source_scan {
    use std::path::{Path, PathBuf};

    /// The `src` tree of every crate in this workspace.
    ///
    /// Relative to this crate, because that is the only anchor a test has.
    /// A crate that is added and not listed here is invisible to every scan
    /// built on this, so the callers assert on the file count as well.
    pub fn crate_source_dirs() -> Vec<PathBuf> {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates/reborn-core has a parent")
            .to_path_buf();
        ["reborn-core", "reborn-ui", "reborn-cli"]
            .iter()
            .map(|name| crates.join(name).join("src"))
            .collect()
    }

    /// Every `.rs` file under every crate in this workspace.
    pub fn workspace_rust_files() -> Vec<PathBuf> {
        let mut out = Vec::new();
        for dir in crate_source_dirs() {
            assert!(
                dir.is_dir(),
                "{} is not a directory, so a whole crate would be scanned silently as nothing",
                dir.display()
            );
            collect_rust_files(&dir, &mut out);
        }
        out
    }

    fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(listing) = std::fs::read_dir(dir) else {
            return;
        };
        for item in listing.flatten() {
            let path = item.path();
            if path.is_dir() {
                collect_rust_files(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }

    /// `text` with every comment, string literal and character literal blanked
    /// to spaces, byte for byte.
    ///
    /// Same length as the input and still valid UTF-8, so an offset found in
    /// the result indexes the original. Search this for structure, and slice
    /// the original for content: a scan that needs to read what a message
    /// SAYS still gets the real text, while brackets, keywords and needles
    /// inside that message are invisible to the parsing.
    ///
    /// Handles line comments, `"..."`, `r"..."`, `r#"..."#` at any hash depth,
    /// their `b` prefixed forms, and character literals. A `'` that begins a
    /// lifetime is left alone. There are no block comments in this workspace
    /// and none are handled: one would be blanked only from its `/*` to the
    /// end of that line.
    pub fn code_only(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = text.to_string().into_bytes();
        let mut i = 0usize;
        let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
            for b in out.iter_mut().take(to).skip(from) {
                if *b != b'\n' {
                    *b = b' ';
                }
            }
        };
        while i < bytes.len() {
            // A line comment: to the end of the line.
            if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
                let end = text[i..].find('\n').map(|n| i + n).unwrap_or(bytes.len());
                blank(&mut out, i, end);
                i = end;
                continue;
            }
            // A raw string, with or without a byte prefix and at any hash
            // depth: `r"..."`, `br##"..."##`.
            let raw_at = if bytes[i] == b'r' {
                Some(i + 1)
            } else if bytes[i] == b'b' && bytes.get(i + 1) == Some(&b'r') {
                Some(i + 2)
            } else {
                None
            };
            if let Some(after_r) = raw_at {
                // Not a raw string if what precedes it makes this an
                // identifier such as `for` or `number`.
                let preceded_by_word = i > 0
                    && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
                let mut hashes = after_r;
                while bytes.get(hashes) == Some(&b'#') {
                    hashes += 1;
                }
                if !preceded_by_word && bytes.get(hashes) == Some(&b'"') {
                    let fence: String =
                        std::iter::once('"').chain(std::iter::repeat_n('#', hashes - after_r)).collect();
                    let end = text[hashes + 1..]
                        .find(&fence)
                        .map(|n| hashes + 1 + n + fence.len())
                        .unwrap_or(bytes.len());
                    blank(&mut out, i, end);
                    i = end;
                    continue;
                }
            }
            // An ordinary string, with or without a byte prefix.
            if bytes[i] == b'"' || (bytes[i] == b'b' && bytes.get(i + 1) == Some(&b'"')) {
                let open = if bytes[i] == b'"' { i } else { i + 1 };
                let mut j = open + 1;
                while j < bytes.len() {
                    match bytes[j] {
                        b'\\' => j += 2,
                        b'"' => break,
                        _ => j += 1,
                    }
                }
                let end = (j + 1).min(bytes.len());
                blank(&mut out, i, end);
                i = end;
                continue;
            }
            // A character literal, but never a lifetime. `'a` and `'static`
            // are followed by an identifier and no closing quote.
            if bytes[i] == b'\'' {
                let closes_at = if bytes.get(i + 1) == Some(&b'\\') {
                    // `'\n'`, `'\\'`, `'\u{1F}'`: to the next quote.
                    text[i + 2..].find('\'').map(|n| i + 2 + n)
                } else {
                    // One character, whatever its width.
                    text[i + 1..]
                        .chars()
                        .next()
                        .map(|c| i + 1 + c.len_utf8())
                        .filter(|at| bytes.get(*at) == Some(&b'\''))
                };
                if let Some(close) = closes_at {
                    blank(&mut out, i, close + 1);
                    i = close + 1;
                    continue;
                }
            }
            i += 1;
        }
        String::from_utf8(out).expect("blanking whole characters keeps the text valid")
    }

    /// `text` with every `#[cfg(test)]` module blanked to spaces.
    ///
    /// Same length as the input, so it composes with [`code_only`] and with
    /// offsets taken from either. Blanked rather than cut at the first one:
    /// `main.rs` alone has six, and the file's real production code carries on
    /// below several of them, so cutting at the first hides it.
    ///
    /// An `#[cfg(test)]` written inside a doc comment or a string is not one,
    /// and is not matched: the search runs over the blanked view.
    pub fn production_only(text: &str) -> String {
        let code = code_only(text);
        let mut out = text.to_string().into_bytes();
        let bytes = code.as_bytes();
        let mut from = 0usize;
        while let Some(found) = code[from..].find("#[cfg(test)]") {
            let at = from + found;
            let mut i = at + "#[cfg(test)]".len();
            // Optional visibility, then `mod`. Anything else is a
            // `#[cfg(test)]` on something that is not a module, and this
            // deliberately leaves it alone.
            while bytes.get(i).is_some_and(|b| b.is_ascii_whitespace()) {
                i += 1;
            }
            if code[i..].starts_with("pub") {
                i += 3;
                if bytes.get(i) == Some(&b'(') {
                    match closing_bracket(&code, i) {
                        Some(close) => i = close + 1,
                        None => break,
                    }
                }
                while bytes.get(i).is_some_and(|b| b.is_ascii_whitespace()) {
                    i += 1;
                }
            }
            if !code[i..].starts_with("mod ") && !code[i..].starts_with("mod\n") {
                from = at + 1;
                continue;
            }
            let Some(open) = code[i..].find('{').map(|n| i + n) else {
                break;
            };
            let Some(close) = closing_bracket(&code, open) else {
                break;
            };
            for b in out.iter_mut().take(close + 1).skip(at) {
                if *b != b'\n' {
                    *b = b' ';
                }
            }
            from = close + 1;
        }
        String::from_utf8(out).expect("blanking whole characters keeps the text valid")
    }

    /// The offset of the bracket that closes the one at `open`, counting only
    /// brackets that are really code.
    ///
    /// `code` must be [`code_only`] output. Handles `(`, `[` and `{`. This is
    /// the whole point of the blanked view: a message containing an
    /// unbalanced bracket, which several on this branch do, used to make the
    /// count run away and truncate everything after it.
    pub fn closing_bracket(code: &str, open: usize) -> Option<usize> {
        let bytes = code.as_bytes();
        let (opener, closer) = match bytes.get(open)? {
            b'(' => (b'(', b')'),
            b'[' => (b'[', b']'),
            b'{' => (b'{', b'}'),
            _ => return None,
        };
        let mut depth = 0usize;
        for (i, b) in bytes.iter().enumerate().skip(open) {
            if *b == opener {
                depth += 1;
            } else if *b == closer {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Every offset in `code` where `needle` really appears as code.
    pub fn code_matches(code: &str, needle: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(found) = code[from..].find(needle) {
            out.push(from + found);
            from += found + 1;
        }
        out
    }

    /// The text a `const NAME: &str = "...";` declaration holds, wherever in
    /// the workspace it is declared.
    ///
    /// Line continuations are honored the way the compiler honors them, so a
    /// constant written across four lines is compared as the one sentence it
    /// really is. `None` when there is no such constant, which the callers
    /// treat as a failure rather than as nothing to check.
    pub fn const_str_value(name: &str) -> Option<String> {
        for file in workspace_rust_files() {
            let text = std::fs::read_to_string(&file).ok()?;
            let code = code_only(&text);
            for pattern in [
                format!("const {name}: &str ="),
                format!("const {name}: &'static str ="),
            ] {
                let Some(at) = code.find(&pattern) else {
                    continue;
                };
                let open = text[at..].find('"').map(|n| at + n)?;
                return Some(unescape_from(&text, open));
            }
        }
        None
    }

    /// The value of the string literal that opens at `open`.
    fn unescape_from(text: &str, open: usize) -> String {
        let bytes = text.as_bytes();
        let mut out = String::new();
        let mut i = open + 1;
        while i < bytes.len() {
            match bytes[i] {
                b'"' => break,
                b'\\' => {
                    match bytes.get(i + 1) {
                        // A continuation: the newline and the indent that
                        // follows it are not part of the value.
                        Some(b'\n') | Some(b'\r') => {
                            i += 1;
                            while bytes.get(i).is_some_and(|b| b.is_ascii_whitespace()) {
                                i += 1;
                            }
                            continue;
                        }
                        Some(b'n') => out.push('\n'),
                        Some(b't') => out.push('\t'),
                        Some(other) => out.push(*other as char),
                        None => break,
                    }
                    i += 2;
                }
                _ => {
                    let c = text[i..].chars().next().expect("a character boundary");
                    out.push(c);
                    i += c.len_utf8();
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retired league uploader is gone, and so is its credential.
    ///
    /// `league.rs` was the launcher-side port of Dolphin's `NflsStatUpload`,
    /// posting to `ingest.php` with a bearer token. `backend.rs` replaced it,
    /// and by the end of this branch nothing called anything in it: it was
    /// reachable only from its own `pub mod` line. Dead code that still
    /// compiles is dead code somebody will one day call.
    ///
    /// The credential is the part that mattered. This branch removed the
    /// commands that could forget the old league token, so any installation
    /// that ever stored one would keep it in the Windows credential vault
    /// forever with no way to remove it from the product. That is a token this
    /// project has already had to rotate.
    #[test]
    fn the_retired_league_uploader_is_gone_and_its_credential_is_named_for_removal() {
        // The declarations only. This test's own text mentions the line it is
        // looking for, so reading the whole file would find itself.
        let lib = include_str!("lib.rs");
        let declarations = &lib[..lib.find("#[cfg(test)]").unwrap_or(lib.len())];
        assert!(
            !declarations.contains("pub mod league;"),
            "the retired league uploader is still declared"
        );
        let file = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/league.rs"));
        assert!(
            !file.exists(),
            "src/league.rs is still on disk, so it is still one line from being called again"
        );
        assert_eq!(
            identity::RETIRED_LEAGUE_TOKEN_ACCOUNT,
            "nsr.league.ingest",
            "the account the retired token was stored under has to keep its exact name, \
             or the cleanup deletes nothing"
        );
    }

    /// The reading every source-scanning test on this branch is built on.
    ///
    /// Two rules that matter for whether a credential reaches a player's disk
    /// now depend on this parser, so it is held to its own cases rather than
    /// trusted. Each half is one of the ways an earlier scan was wrong.
    #[test]
    fn the_source_scan_reads_code_and_not_what_only_looks_like_code() {
        use super::source_scan::{closing_bracket, code_only, production_only};

        // A bracket inside a message is not a bracket. Counting them was how
        // one scan decided a call ended in the middle of its own message and
        // never looked at the argument after it.
        let call = "f(\"a stray ) in a sentence\", token)";
        let code = code_only(call);
        assert_eq!(code.len(), call.len(), "the blanked view has to line up with the text");
        assert_eq!(
            closing_bracket(&code, 1),
            Some(call.len() - 1),
            "a bracket inside a string literal was counted as code"
        );
        assert!(
            !code.contains("stray"),
            "a string literal's contents were left in the blanked view"
        );

        // A keyword inside a comment or a literal is not a keyword either.
        let quoted = "let a = \"#[cfg(test)]\"; // #[cfg(test)]\nfn real() {}";
        assert!(
            production_only(quoted).contains("fn real"),
            "an #[cfg(test)] written inside a string or a comment was taken for a test module"
        );

        // Every test module, not the text from the first one onwards.
        let two = "fn a() {}\n#[cfg(test)]\nmod t1 { fn x() { let s = \"}\"; } }\n\
                   fn between() {}\n#[cfg(test)]\nmod t2 { }\nfn after() {}";
        let production = production_only(two);
        for kept in ["fn a", "fn between", "fn after"] {
            assert!(
                production.contains(kept),
                "{kept} was blanked with the test modules, so production code went unscanned"
            );
        }
        for dropped in ["mod t1", "mod t2"] {
            assert!(
                !production.contains(dropped),
                "{dropped} survived, so a test's own words are part of what every scan reads"
            );
        }

        // And on the real file that has the most of them: `main.rs` closes its
        // first test module and then declares `fn main`, which the cut-at-the-
        // first-one reading never saw.
        let main_rs = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("crates/")
                .join("reborn-ui")
                .join("src")
                .join("main.rs"),
        )
        .expect("the window crate's main.rs");
        let production = production_only(&main_rs);
        assert!(
            production.contains("\nfn main() {"),
            "the window's own entry point sits after an inline test module and was not read"
        );
        assert!(
            !production.contains("mod shortcut_tests"),
            "an inline test module was left in what every scan reads"
        );
    }

    #[test]
    fn every_data_dir_has_a_distinct_relative_path() {
        let mut seen = std::collections::HashSet::new();
        for dir in DataDir::ALL {
            assert!(
                seen.insert(dir.relative()),
                "duplicate relative path: {}",
                dir.relative()
            );
        }
        assert_eq!(seen.len(), DataDir::ALL.len());
    }

    #[test]
    fn data_dir_paths_are_relative_and_use_forward_slashes() {
        for dir in DataDir::ALL {
            let rel = dir.relative();
            assert!(!rel.starts_with('/'), "{rel} must be relative");
            assert!(!rel.contains('\\'), "{rel} must not hardcode a separator");
            assert!(!rel.contains(':'), "{rel} must not contain a drive letter");
        }
    }

    /// User data and derived data must never overlap. If a directory were both,
    /// a cache purge could delete somebody's memory cards.
    #[test]
    fn user_data_is_never_also_derived() {
        for dir in DataDir::ALL {
            assert!(
                !(dir.is_user_data() && dir.is_derived()),
                "{:?} is classed as both user data and derived",
                dir
            );
        }
    }

    #[test]
    fn saves_are_user_data_and_never_purgeable() {
        assert!(DataDir::Saves.is_user_data());
        assert!(!DataDir::Saves.is_derived());
        assert!(DataDir::MusicSources.is_user_data());
        assert!(!DataDir::MusicSources.is_derived());
    }

    /// The bug this whole layout exists to prevent: the shipped 2.1 uninstaller
    /// deletes the install root, and the user's memory cards live inside it.
    #[test]
    fn install_root_never_contains_the_data_root() {
        let p = Platform::detect();
        let install = p.paths.install_root().expect("install root");
        let data = p.paths.data_root().expect("data root");
        assert!(
            !data.starts_with(&install),
            "data root {} is inside install root {}, which is how uninstalling deletes saves",
            data.display(),
            install.display()
        );
    }

    #[test]
    fn data_root_is_absolute_and_named_for_the_product() {
        let p = Platform::detect();
        let data = p.paths.data_root().expect("data root");
        assert!(data.is_absolute(), "{} must be absolute", data.display());
        assert!(
            data.to_string_lossy().contains("NFL Street Reborn"),
            "{} should be under a product-named directory",
            data.display()
        );
    }

    #[test]
    fn exe_suffix_matches_the_platform() {
        let p = Platform::detect();
        if cfg!(windows) {
            assert_eq!(p.paths.exe_suffix(), ".exe");
        } else {
            assert_eq!(p.paths.exe_suffix(), "");
        }
    }

    #[test]
    fn a_clean_exit_is_not_a_crash() {
        assert!(!ExitKind::Normal.is_crash());
        assert!(!ExitKind::Terminated.is_crash());
        assert!(!ExitKind::Error { code: 1 }.is_crash());
        assert!(ExitKind::Crashed {
            detail: "access violation".into()
        }
        .is_crash());
    }

    #[test]
    fn same_volume_agrees_with_itself() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = Platform::detect();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        std::fs::write(&a, b"a").unwrap();
        assert!(
            p.files.same_volume(&a, &b).expect("same_volume"),
            "two paths in one directory must be on one volume"
        );
    }

    #[test]
    fn atomic_replace_moves_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = Platform::detect();
        let src = dir.path().join("new");
        let dest = dir.path().join("live");
        std::fs::write(&src, b"new contents").unwrap();
        std::fs::write(&dest, b"old contents").unwrap();

        p.files.atomic_replace(&src, &dest).expect("replace");

        assert_eq!(std::fs::read(&dest).unwrap(), b"new contents");
        assert!(!src.exists(), "source should be gone after a rename");
    }

    #[test]
    fn hardlink_shares_content_or_reports_why_not() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = Platform::detect();
        let src = dir.path().join("pristine.dds");
        let dest = dir.path().join("composed").join("pristine.dds");
        std::fs::write(&src, b"texture").unwrap();

        // Composition falls back to copying on filesystems without hardlinks,
        // notably exFAT and FAT32, so a failure here is a supported outcome
        // rather than a test failure.
        match p.files.hardlink(&src, &dest) {
            Ok(()) => assert_eq!(std::fs::read(&dest).unwrap(), b"texture"),
            Err(e) => eprintln!("hardlink unsupported here, composition would copy: {e}"),
        }
    }

    #[test]
    fn spawning_and_waiting_classifies_a_clean_exit() {
        let p = Platform::detect();
        let (program, args): (&str, Vec<std::ffi::OsString>) = if cfg!(windows) {
            ("cmd", vec!["/C".into(), "exit 0".into()])
        } else {
            ("sh", vec!["-c".into(), "exit 0".into()])
        };
        let handle = p
            .processes
            .spawn(
                std::path::Path::new(program),
                &args,
                None,
                &Default::default(),
            )
            .expect("spawn");
        assert!(handle.pid() > 0);
        assert_eq!(p.processes.wait(handle).expect("wait"), ExitKind::Normal);
    }

    #[test]
    fn a_nonzero_exit_is_an_error_not_a_crash() {
        let p = Platform::detect();
        let (program, args): (&str, Vec<std::ffi::OsString>) = if cfg!(windows) {
            ("cmd", vec!["/C".into(), "exit 3".into()])
        } else {
            ("sh", vec!["-c".into(), "exit 3".into()])
        };
        let handle = p
            .processes
            .spawn(
                std::path::Path::new(program),
                &args,
                None,
                &Default::default(),
            )
            .expect("spawn");
        let kind = p.processes.wait(handle).expect("wait");
        assert_eq!(kind, ExitKind::Error { code: 3 });
        assert!(!kind.is_crash(), "exit 3 is a failure, not a crash");
    }

    /// The credential vault must really persist, and must never fall back
    /// to plaintext. A vault that silently does not store is a security bug
    /// that passes tests, so this stores, reads back, deletes, and checks the
    /// value is gone rather than asserting any of it.
    ///
    /// Skipped rather than failed when no platform store is available, since
    /// a headless CI box legitimately has none and that is not this crate's
    /// fault. The skip is printed, so it cannot pass quietly forever.
    #[test]
    fn the_credential_vault_persists_and_deletes() {
        let p = Platform::detect();
        let name = "nsr-launcher-selftest";
        let secret = "value-that-must-round-trip";

        match p.credentials.store(name, secret) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("SKIPPED: no usable credential store here: {e}");
                return;
            }
        }

        let got = p
            .credentials
            .retrieve(name)
            .expect("a stored secret must be retrievable");
        assert_eq!(
            got.as_deref(),
            Some(secret),
            "the vault did not return what was stored"
        );

        p.credentials.delete(name).expect("delete must succeed");
        assert!(
            p.credentials
                .retrieve(name)
                .map(|v| v.is_none())
                .unwrap_or(true),
            "a deleted secret is still readable"
        );
    }

    /// Drive enumeration must not report a fixed disk as removable. The Xbox
    /// wizard copies to whatever this returns, and picking the system drive
    /// by mistake is the failure worth preventing.
    #[test]
    fn drive_enumeration_never_calls_a_fixed_disk_removable() {
        let p = Platform::detect();
        let drives = match p.drives.enumerate() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("SKIPPED: drive enumeration unavailable here: {e}");
                return;
            }
        };

        // enumerate() returns ONLY removable volumes by construction, so the
        // system drive appearing here at all is the failure, not a flag on it.
        if let Ok(exe) = std::env::current_exe() {
            for d in &drives {
                assert!(
                    !exe.starts_with(&d.mount_point),
                    "{:?} holds the running executable and must never be offered as removable",
                    d.mount_point
                );
            }
        }
        eprintln!("enumerated {} drive(s)", drives.len());
    }
}

pub mod dolphin_preferences;
pub mod live_plays;
pub mod live_view;
pub mod matchmaking;
pub mod presence;
pub mod presence_discord;
pub mod relay_driver;
pub mod series;
pub mod traversal_probe;
