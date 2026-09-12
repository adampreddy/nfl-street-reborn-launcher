//! Editing an INI file without destroying the parts we do not own.
//!
//! # The bug this exists to fix
//!
//! The launcher generated whole configuration files from scratch on every
//! launch, on the stated theory that regenerating means the launcher stays the
//! single source of truth. That theory holds for the dozen keys the launcher
//! actually decides and is destructive for everything else.
//!
//! Measured on a real install: the generated `Dolphin.ini` was 18 lines across
//! 6 sections. The player's own was 215 lines across 16, including `[Core]`,
//! `[GameList]`, `[Input]`, `[Movie]` and `[FifoPlayer]`. Replacing the file
//! would have silently deleted ten sections of tuned settings, among them the
//! paths to their own game images.
//!
//! So the launcher no longer writes files. It writes **keys**, and everything
//! it has no opinion about survives byte for byte.
//!
//! # What is preserved
//!
//! Comments, blank lines, section order, key order, and unusual spelling of
//! the original. A merge that changes nothing rewrites nothing: the round trip
//! of an untouched document is byte identical, which matters because a
//! launcher that reformats a config on every start looks like it is corrupting
//! it.

use std::collections::BTreeMap;

/// One line of an INI document, kept in its original form.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    /// `[Section]`
    Section { name: String, raw: String },
    /// `key = value`
    Pair {
        key: String,
        raw: String,
        /// The exact separator used, so `a=b` is not rewritten as `a = b`.
        sep: String,
    },
    /// A comment, a blank line, or anything unparsed. Never touched.
    Other(String),
}

/// An INI document that can be edited in place.
#[derive(Debug, Clone)]
pub struct IniDocument {
    lines: Vec<Line>,
    /// The newline the file used, so a CRLF file stays CRLF.
    newline: String,
    /// Whether the original ended with a newline.
    trailing_newline: bool,
}

/// The keys a caller wants set, as section then key then value.
pub type Overlay = BTreeMap<String, BTreeMap<String, String>>;

/// What a merge changed, so a caller can report it honestly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Keys whose value was changed.
    pub updated: Vec<String>,
    /// Keys that did not exist and were added.
    pub added: Vec<String>,
    /// Sections that did not exist and were created.
    pub sections_created: Vec<String>,
    /// Keys already holding the wanted value, so nothing was written.
    pub unchanged: Vec<String>,
}

impl MergeReport {
    /// Whether the document actually needs writing back.
    ///
    /// Used to skip the write entirely when nothing changed, which is what
    /// keeps timestamps and file contents stable across launches.
    pub fn is_noop(&self) -> bool {
        self.updated.is_empty() && self.added.is_empty() && self.sections_created.is_empty()
    }
}

impl IniDocument {
    /// Parse a document. Never fails: anything unrecognised is preserved
    /// verbatim as an untouched line, because refusing to load a config the
    /// player has hand edited would be worse than not understanding part of it.
    pub fn parse(text: &str) -> Self {
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let trailing_newline = text.ends_with('\n');

        let mut lines = Vec::new();
        for raw in text.split_inclusive('\n') {
            let stripped = raw.trim_end_matches(['\r', '\n']);
            let trimmed = stripped.trim();

            if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
                lines.push(Line::Section {
                    name: trimmed[1..trimmed.len() - 1].trim().to_string(),
                    raw: stripped.to_string(),
                });
                continue;
            }

            // A comment must never be read as a pair, or a commented-out
            // setting would be silently activated by a merge.
            if trimmed.starts_with('#') || trimmed.starts_with(';') || trimmed.is_empty() {
                lines.push(Line::Other(stripped.to_string()));
                continue;
            }

            if let Some(eq) = stripped.find('=') {
                let key = stripped[..eq].trim().to_string();
                if !key.is_empty() {
                    // Capture the separator exactly, so `a=b` stays `a=b`.
                    let left_pad: String = stripped[..eq]
                        .chars()
                        .rev()
                        .take_while(|c| c.is_whitespace())
                        .collect();
                    let right_pad: String = stripped[eq + 1..]
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect();
                    lines.push(Line::Pair {
                        key,
                        raw: stripped.to_string(),
                        sep: format!("{left_pad}={right_pad}"),
                    });
                    continue;
                }
            }

            lines.push(Line::Other(stripped.to_string()));
        }

        Self {
            lines,
            newline: newline.to_string(),
            trailing_newline,
        }
    }

    /// Read a value, if it is set.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let mut current: Option<&str> = None;
        for line in &self.lines {
            match line {
                Line::Section { name, .. } => current = Some(name),
                Line::Pair { key: k, raw, sep } if current == Some(section) && k == key => {
                    let idx = raw.find(sep.trim_end())?;
                    let after = &raw[idx + sep.len()..];
                    return Some(after);
                }
                _ => {}
            }
        }
        None
    }

    /// Set the keys in `overlay`, leaving everything else exactly as it was.
    pub fn merge(&mut self, overlay: &Overlay) -> MergeReport {
        let mut report = MergeReport::default();

        for (section, keys) in overlay {
            for (key, value) in keys {
                let label = format!("{section}/{key}");
                match self.set_existing(section, key, value) {
                    SetResult::Updated => report.updated.push(label),
                    SetResult::AlreadyCorrect => report.unchanged.push(label),
                    SetResult::NoSuchKey => {
                        if self.append_to_section(section, key, value) {
                            report.added.push(label);
                        } else {
                            self.create_section(section, key, value);
                            if !report.sections_created.contains(section) {
                                report.sections_created.push(section.clone());
                            }
                            report.added.push(label);
                        }
                    }
                }
            }
        }

        report
    }

    /// Take one key out of one section. Everything else, comments and blank
    /// lines included, stays exactly as it was. Returns whether a pair was
    /// removed.
    pub fn remove(&mut self, section: &str, key: &str) -> bool {
        let mut current: Option<String> = None;
        let mut index = None;
        for (i, line) in self.lines.iter().enumerate() {
            match line {
                Line::Section { name, .. } => current = Some(name.clone()),
                Line::Pair { key: k, .. } if current.as_deref() == Some(section) && k == key => {
                    index = Some(i);
                    break;
                }
                _ => {}
            }
        }
        match index {
            Some(i) => {
                self.lines.remove(i);
                true
            }
            None => false,
        }
    }

    fn set_existing(&mut self, section: &str, key: &str, value: &str) -> SetResult {
        let mut current: Option<String> = None;
        for line in self.lines.iter_mut() {
            match line {
                Line::Section { name, .. } => current = Some(name.clone()),
                Line::Pair {
                    key: k, raw, sep, ..
                } if current.as_deref() == Some(section) && k == key => {
                    let wanted = format!("{k}{sep}{value}");
                    if *raw == wanted {
                        return SetResult::AlreadyCorrect;
                    }
                    *raw = wanted;
                    return SetResult::Updated;
                }
                _ => {}
            }
        }
        SetResult::NoSuchKey
    }

    /// Append a key to an existing section. Returns false if the section is
    /// absent.
    fn append_to_section(&mut self, section: &str, key: &str, value: &str) -> bool {
        let start = self
            .lines
            .iter()
            .position(|l| matches!(l, Line::Section { name, .. } if name == section));
        let Some(start) = start else { return false };

        // Insert after the last pair in this section, before any trailing
        // blank lines, so a new key does not land after a comment that belongs
        // to the next section.
        let mut insert_at = start + 1;
        let mut last_pair = start;
        for (i, line) in self.lines.iter().enumerate().skip(start + 1) {
            match line {
                Line::Section { .. } => break,
                Line::Pair { .. } => {
                    last_pair = i;
                    insert_at = i + 1;
                }
                Line::Other(_) => {}
            }
        }
        if last_pair > start {
            insert_at = last_pair + 1;
        }

        self.lines.insert(
            insert_at,
            Line::Pair {
                key: key.to_string(),
                raw: format!("{key} = {value}"),
                sep: " = ".to_string(),
            },
        );
        true
    }

    fn create_section(&mut self, section: &str, key: &str, value: &str) {
        // A blank line before a new section, unless the document is empty or
        // already ends blank.
        let needs_gap = !self.lines.is_empty()
            && !matches!(self.lines.last(), Some(Line::Other(s)) if s.trim().is_empty());
        if needs_gap {
            self.lines.push(Line::Other(String::new()));
        }
        self.lines.push(Line::Section {
            name: section.to_string(),
            raw: format!("[{section}]"),
        });
        self.lines.push(Line::Pair {
            key: key.to_string(),
            raw: format!("{key} = {value}"),
            sep: " = ".to_string(),
        });
    }

    /// Serialise back. An untouched document round trips byte for byte.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            let text = match line {
                Line::Section { raw, .. } => raw,
                Line::Pair { raw, .. } => raw,
                Line::Other(raw) => raw,
            };
            out.push_str(text);
            let last = i + 1 == self.lines.len();
            if !last || self.trailing_newline {
                out.push_str(&self.newline);
            }
        }
        out
    }
}

/// Provided so `to_string()` exists in the ordinary way, rather than as an
/// inherent method shadowing it.
impl std::fmt::Display for IniDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

enum SetResult {
    Updated,
    AlreadyCorrect,
    NoSuchKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_takes_one_key_out_and_leaves_everything_else_byte_for_byte() {
        let text = "[General]\r\nLoadPath = User/Load///\r\nGDBPort = -1\r\n[Core]\r\nSlotA = 255\r\n";
        let mut doc = IniDocument::parse(text);
        assert!(doc.remove("General", "LoadPath"));
        assert!(!doc.remove("General", "LoadPath"), "gone already");
        assert!(!doc.remove("Nope", "GDBPort"), "wrong section");
        assert_eq!(doc.render(), "[General]\r\nGDBPort = -1\r\n[Core]\r\nSlotA = 255\r\n");
        let untouched = IniDocument::parse(text);
        assert_eq!(untouched.render(), text);
    }

    fn overlay(items: &[(&str, &str, &str)]) -> Overlay {
        let mut o = Overlay::new();
        for (s, k, v) in items {
            o.entry(s.to_string())
                .or_default()
                .insert(k.to_string(), v.to_string());
        }
        o
    }

    /// The whole reason this module exists. A real Dolphin.ini has sections the
    /// launcher has no opinion about, and losing them loses the player's tuned
    /// emulation settings and the paths to their own game images.
    #[test]
    fn sections_we_do_not_touch_survive_exactly() {
        let src = "[Core]\nCPUThread = True\nSyncGPU = False\n\n\
                   [GameList]\nISOPath0 = D:\\games\n\n\
                   [Display]\nFullscreen = False\n";
        let mut d = IniDocument::parse(src);
        d.merge(&overlay(&[("Display", "Fullscreen", "True")]));
        let out = d.render();

        assert!(out.contains("CPUThread = True"), "{out}");
        assert!(out.contains("SyncGPU = False"), "{out}");
        assert!(out.contains("ISOPath0 = D:\\games"), "{out}");
        assert!(out.contains("Fullscreen = True"), "{out}");
        assert!(!out.contains("Fullscreen = False"), "{out}");
    }

    /// A merge that changes nothing must rewrite nothing. A launcher that
    /// reformats a config on every start looks like it is corrupting it.
    #[test]
    fn an_untouched_document_round_trips_byte_for_byte() {
        let src = "; a comment\n[Core]\nCPUThread=True\n\n\n[Display]\n  Fullscreen  =  False  \n";
        let d = IniDocument::parse(src);
        assert_eq!(d.render(), src);
    }

    #[test]
    fn setting_a_value_that_is_already_correct_is_a_noop() {
        let src = "[Display]\nFullscreen = True\n";
        let mut d = IniDocument::parse(src);
        let r = d.merge(&overlay(&[("Display", "Fullscreen", "True")]));
        assert!(r.is_noop(), "{r:?}");
        assert_eq!(d.render(), src);
    }

    /// Updating must replace in place, never append, or the file grows a
    /// duplicate key every launch and which one wins becomes parser-dependent.
    #[test]
    fn updating_replaces_in_place_and_never_duplicates() {
        let src = "[Display]\nFullscreen = False\nRenderToMain = True\n";
        let mut d = IniDocument::parse(src);
        for _ in 0..5 {
            d.merge(&overlay(&[("Display", "Fullscreen", "True")]));
        }
        let out = d.render();
        assert_eq!(out.matches("Fullscreen").count(), 1, "{out}");
        assert!(out.contains("RenderToMain = True"), "{out}");
    }

    #[test]
    fn a_new_key_joins_the_existing_section() {
        let src = "[Display]\nFullscreen = True\n\n[Core]\nCPUThread = True\n";
        let mut d = IniDocument::parse(src);
        let r = d.merge(&overlay(&[("Display", "RenderToMain", "True")]));
        assert_eq!(r.added, vec!["Display/RenderToMain"]);
        let out = d.render();
        // It must land inside [Display], not after [Core].
        let disp = out.find("[Display]").unwrap();
        let core = out.find("[Core]").unwrap();
        let added = out.find("RenderToMain").unwrap();
        assert!(
            disp < added && added < core,
            "landed in the wrong section:\n{out}"
        );
    }

    #[test]
    fn a_missing_section_is_created() {
        let src = "[Core]\nCPUThread = True\n";
        let mut d = IniDocument::parse(src);
        let r = d.merge(&overlay(&[("Logger", "WriteToFile", "False")]));
        assert_eq!(r.sections_created, vec!["Logger"]);
        let out = d.render();
        assert!(out.contains("[Logger]"), "{out}");
        assert!(out.contains("WriteToFile = False"), "{out}");
        assert!(out.contains("CPUThread = True"), "{out}");
    }

    /// A commented-out setting must not be read as a live one, or a merge would
    /// silently activate something the player deliberately disabled.
    #[test]
    fn a_commented_setting_is_not_treated_as_a_pair() {
        let src = "[Core]\n; CPUThread = False\n# SyncGPU = False\n";
        let mut d = IniDocument::parse(src);
        assert_eq!(d.get("Core", "CPUThread"), None);
        let r = d.merge(&overlay(&[("Core", "CPUThread", "True")]));
        assert_eq!(r.added, vec!["Core/CPUThread"]);
        let out = d.render();
        assert!(out.contains("; CPUThread = False"), "comment lost:\n{out}");
        assert!(out.contains("CPUThread = True"), "{out}");
    }

    /// A CRLF file must stay CRLF. Rewriting line endings on a config makes
    /// every line show as changed in any diff the player looks at.
    #[test]
    fn line_endings_are_preserved() {
        let src = "[Display]\r\nFullscreen = False\r\n";
        let mut d = IniDocument::parse(src);
        d.merge(&overlay(&[("Display", "Fullscreen", "True")]));
        let out = d.render();
        assert!(out.contains("\r\n"), "lost CRLF");
        assert!(!out.contains("\n\n"), "mixed endings: {out:?}");
    }

    #[test]
    fn unusual_spacing_is_kept_when_updating() {
        let src = "[Display]\nFullscreen=False\n";
        let mut d = IniDocument::parse(src);
        d.merge(&overlay(&[("Display", "Fullscreen", "True")]));
        assert_eq!(d.render(), "[Display]\nFullscreen=True\n");
    }

    #[test]
    fn an_empty_document_gains_what_it_needs() {
        let mut d = IniDocument::parse("");
        d.merge(&overlay(&[("Display", "Fullscreen", "True")]));
        let out = d.render();
        assert!(out.contains("[Display]"), "{out}");
        assert!(out.contains("Fullscreen = True"), "{out}");
    }

    /// The same key in two different sections is two different settings.
    #[test]
    fn keys_are_scoped_to_their_section() {
        let src = "[A]\nEnabled = False\n\n[B]\nEnabled = False\n";
        let mut d = IniDocument::parse(src);
        d.merge(&overlay(&[("B", "Enabled", "True")]));
        let out = d.render();
        let a = out.find("[A]").unwrap();
        let b = out.find("[B]").unwrap();
        assert!(
            out[a..b].contains("Enabled = False"),
            "A was changed:\n{out}"
        );
        assert!(
            out[b..].contains("Enabled = True"),
            "B was not changed:\n{out}"
        );
    }

    #[test]
    fn reading_back_a_value_works() {
        let src = "[Display]\nFullscreen = True\nScale=2\n";
        let d = IniDocument::parse(src);
        assert_eq!(d.get("Display", "Fullscreen"), Some("True"));
        assert_eq!(d.get("Display", "Scale"), Some("2"));
        assert_eq!(d.get("Display", "Absent"), None);
        assert_eq!(d.get("Nope", "Fullscreen"), None);
    }
}
