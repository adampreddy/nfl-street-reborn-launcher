//! Embedded tags and file-name cleaning.
//!
//! Tags are the first source of a song's details, read offline. When a file
//! has none, the cleaned file name becomes the query for the online lookup:
//! the track number, the bracketed noise, and the words a download site
//! appends are stripped so "03. Artist - Title (Official Video) [HD]" asks
//! for "Artist Title".

use std::path::Path;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::Accessor;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
}

fn clean_field(s: Option<std::borrow::Cow<'_, str>>) -> Option<String> {
    let t = s?.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// Read a file's tags. Never errors: a file lofty cannot read yields an
/// empty `Tags`, and the caller falls through to the file name.
pub fn read(path: &Path) -> Tags {
    let Ok(file) = lofty::read_from_path(path) else {
        return Tags::default();
    };
    let duration_ms = {
        let d = file.properties().duration();
        if d.is_zero() {
            None
        } else {
            Some(d.as_millis() as u64)
        }
    };
    let tag = file.primary_tag().or_else(|| file.first_tag());
    match tag {
        Some(t) => Tags {
            title: clean_field(t.title()),
            artist: clean_field(t.artist()),
            album: clean_field(t.album()),
            duration_ms,
        },
        None => Tags { duration_ms, ..Tags::default() },
    }
}

/// Words a download site appends that are never part of a song's name.
const NOISE: &[&str] = &["official", "video", "audio", "lyrics", "lyric", "hd", "hq", "remaster", "remastered", "4k"];

/// A search query from a file name (without its extension).
pub fn clean_query(file_stem: &str) -> String {
    // 1. bracketed and parenthesized groups
    let mut s = String::with_capacity(file_stem.len());
    let mut depth_round = 0usize;
    let mut depth_square = 0usize;
    for c in file_stem.chars() {
        match c {
            '(' => depth_round += 1,
            ')' => depth_round = depth_round.saturating_sub(1),
            '[' => depth_square += 1,
            ']' => depth_square = depth_square.saturating_sub(1),
            _ if depth_round == 0 && depth_square == 0 => s.push(c),
            _ => {}
        }
    }
    // 2. separators to spaces
    let s = s.replace('_', " ");
    let s = s.replace(" - ", " ");
    // 3. a leading track number. "01 Scarred", "01. X", "01 - X", "1-X",
    // "1_X" (already a space) drop the number; "50 Cent" and "2Pac" keep it:
    // a bare space only counts when the number has a leading zero.
    let trimmed = s.trim_start();
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    let s = if digits > 0 && digits <= 3 {
        let number = &trimmed[..digits];
        let rest = &trimmed[digits..];
        let punctuated = rest.starts_with('.') || rest.starts_with('-');
        let spaced = rest.starts_with(' ') && (number.starts_with('0') || digits == 1);
        if punctuated || spaced {
            rest.trim_start_matches(['.', '-', ' ']).to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    };
    // 4. noise words, whole words, case-insensitive; the rest joined by one space
    s.split_whitespace()
        .filter(|w| {
            let bare = w.trim_matches(|c: char| !c.is_alphanumeric()).to_ascii_lowercase();
            !NOISE.contains(&bare.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::write_wav_24k;

    #[test]
    fn clean_query_table() {
        let cases = [
            ("03. Alicia Keys - Unthinkable (Official Video) [HD]", "Alicia Keys Unthinkable"),
            ("Drake_-_Headlines_Lyrics", "Drake Headlines"),
            ("Mase feat. P. Diddy - Scarred", "Mase feat. P. Diddy Scarred"),
            ("01 Scarred", "Scarred"),
            ("01. Scarred", "Scarred"),
            ("01 - Scarred", "Scarred"),
            ("1-Scarred", "Scarred"),
            ("1_Scarred", "Scarred"),
            ("Song [Remastered] (HQ Audio)", "Song"),
            ("Ludacris ft. Lil Jon - Get Back  HD", "Ludacris ft. Lil Jon Get Back"),
            ("  spaced   out  ", "spaced out"),
            ("2Pac - Changes", "2Pac Changes"),
            ("50 Cent - In Da Club", "50 Cent In Da Club"),
            ("", ""),
        ];
        for (input, want) in cases {
            assert_eq!(clean_query(input), want, "input {input:?}");
        }
    }

    #[test]
    fn a_wav_has_a_duration_and_no_title() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.wav");
        let samples = vec![0i16; 24_000 * 2];
        write_wav_24k(&path, &samples, &samples);
        let t = read(&path);
        assert_eq!(t.title, None);
        assert_eq!(t.artist, None);
        assert_eq!(t.album, None);
        assert_eq!(t.duration_ms, Some(2000));
        assert_eq!(read(&dir.path().join("missing.mp3")), Tags::default());
    }
}
