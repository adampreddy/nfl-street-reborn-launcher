//! Custom soundtrack: the 21 songs on the NFL Street 2 disc, replaced,
//! shared, and reset from the launcher.
//!
//! Design: docs/superpowers/specs/2026-09-08-music-editor-design.md.
//! Every byte layout here was reverse-engineered and confirmed in game; the
//! Python tools under nflstreet2_clean/tools are the oracle the tests compare
//! against (`NSR_CLEAN` names that tree; the oracle tests skip without it).

pub mod apply;
pub mod asf;
pub mod decode;
pub mod ea_xa;
pub mod eatx;
pub mod gcm;
pub mod lookup;
pub mod pack;
pub mod pathfind;
pub mod resample;
pub mod segments;
pub mod state;
pub mod street1;
pub mod tags;
#[cfg(test)]
pub(crate) mod testing;

/// Songs on the disc. The banner table, the playlist, and the ASF all agree.
pub const SONG_SLOTS: u8 = 21;
/// Streams per song in the ASF. The engine bounds each song to its 40 streams.
pub const SEGMENTS_PER_SONG: usize = 40;
/// SCHl streams in the shipped ASF: 3 leading, 21 x 40, 33 trailing.
pub const STREAM_COUNT: usize = 876;
/// Per-segment byte ceiling: the stream ring buffer is 256 KiB, and
/// extend_slot_40big.py refuses a segment at or past this.
pub const SEGMENT_CEILING: usize = 0x3E000;
/// The game's music sample rate.
pub const SAMPLE_RATE: u32 = 24_000;

/// A song slot, 1..=21. Constructed only through `new`, so nothing downstream
/// re-checks the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot(u8);

impl Slot {
    pub fn new(n: u8) -> Result<Self, Refusal> {
        if (1..=SONG_SLOTS).contains(&n) {
            Ok(Self(n))
        } else {
            Err(Refusal::SlotOutOfRange { slot: n })
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }

    /// Zero-based, for record and table indexing.
    pub fn index(self) -> usize {
        usize::from(self.0 - 1)
    }

    pub fn all() -> impl Iterator<Item = Slot> {
        (1..=SONG_SLOTS).map(Slot)
    }
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Index of the first of a slot's 40 streams in the ASF. Same formula as
/// music_pack.py's `slot_first`.
pub fn slot_first_stream(slot: Slot) -> usize {
    match slot.get() {
        1 => 3,
        n => 44 + (usize::from(n) - 2) * SEGMENTS_PER_SONG,
    }
}

/// A soundtrack operation that will not happen, and is not a fault.
///
/// Each one is a state with a next step, so the page and the CLI show a
/// sentence rather than an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    SlotOutOfRange { slot: u8 },
    SessionActive,
    ImageInUse { path: std::path::PathBuf },
    NoPristine,
    NoBooted,
    SongTooLong { seconds: u64, limit_seconds: u64 },
    UnsupportedFormat { extension: String },
    InvalidPack { path: std::path::PathBuf, reason: String },
    SourceMissing { slot: u8, what: String },
    TooLongText { field: &'static str, len: usize },
    OnlineBuild,
    NothingStaged,
    Disabled,
    /// The staged soundtrack does not fit the disc: the rebuilt image would
    /// be `over_bytes` past the disc's size.
    DiscFull { over_bytes: u64 },
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::SlotOutOfRange { slot } => {
                format!("Slot {slot} does not exist. The soundtrack has slots 1 to {SONG_SLOTS}.")
            }
            Refusal::SessionActive => {
                "The game is running. Close it, then apply your soundtrack.".to_string()
            }
            Refusal::ImageInUse { path } => format!(
                "{} is open in another program, so it was not replaced. Close the game and try again.",
                path.display()
            ),
            Refusal::NoPristine => {
                "There is no imported game image to build from. Import the game first.".to_string()
            }
            Refusal::NoBooted => {
                "There is no game image at library/offline. Import the game first.".to_string()
            }
            Refusal::SongTooLong { seconds, limit_seconds } => format!(
                "This song is {}:{:02} long. Songs up to {}:{:02} fit a slot. Trim it in an audio editor and add it again.",
                seconds / 60,
                seconds % 60,
                limit_seconds / 60,
                limit_seconds % 60
            ),
            Refusal::UnsupportedFormat { extension } => format!(
                "Files ending in .{extension} cannot be read here. Convert it to MP3 first."
            ),
            Refusal::InvalidPack { path, reason } => {
                format!("{} is not a usable soundtrack pack: {reason}", path.display())
            }
            Refusal::SourceMissing { slot, what } => format!(
                "Slot {slot} points at {what}, which is no longer there. Add the song again or reset the slot."
            ),
            Refusal::TooLongText { field, len } => format!(
                "The {field} is {len} characters. The game shows at most 49. Shorten it."
            ),
            Refusal::OnlineBuild => "Custom music applies to the offline game only.".to_string(),
            Refusal::NothingStaged => "Nothing is staged. Add or reset a song first.".to_string(),
            Refusal::Disabled => {
                "The custom soundtrack is switched off. Turn it on to apply songs.".to_string()
            }
            Refusal::DiscFull { over_bytes } => format!(
                "Your soundtrack is about {:.1} MB too big for the disc. Replace a long song with a shorter one, or put an original song back, then apply again.",
                (*over_bytes as f64 / 1_048_576.0).max(0.1)
            ),
        }
    }
}

impl From<Refusal> for crate::platform::Error {
    fn from(r: Refusal) -> Self {
        crate::platform::Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            r.message(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_one_to_twenty_one() {
        assert!(Slot::new(0).is_err());
        assert!(Slot::new(22).is_err());
        assert_eq!(Slot::new(1).unwrap().get(), 1);
        assert_eq!(Slot::new(21).unwrap().index(), 20);
        assert_eq!(Slot::all().count(), 21);
    }

    #[test]
    fn first_stream_matches_the_python_tools() {
        // slot_first(song) = 3 if song == 1 else 44 + (song - 2) * 40
        assert_eq!(slot_first_stream(Slot::new(1).unwrap()), 3);
        assert_eq!(slot_first_stream(Slot::new(2).unwrap()), 44);
        assert_eq!(slot_first_stream(Slot::new(21).unwrap()), 804);
        assert_eq!(slot_first_stream(Slot::new(21).unwrap()) + SEGMENTS_PER_SONG, 844);
    }

    #[test]
    fn refusals_read_as_sentences_with_a_next_step() {
        let m = Refusal::SongTooLong { seconds: 401, limit_seconds: 380 }.message();
        assert!(m.contains("6:41") && m.contains("6:20"), "{m}");
        assert!(Refusal::SlotOutOfRange { slot: 0 }.message().contains("1 to 21"));
    }
}
