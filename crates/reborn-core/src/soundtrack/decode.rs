//! Audio file decode (symphonia) to 24 kHz stereo PCM.
//!
//! The formats a player can add are the ones symphonia decodes in pure
//! Rust: mp3, aac in m4a, flac, vorbis in ogg, wav, aiff. Anything else is
//! refused by extension before a byte is read, so the refusal names the
//! file type rather than a codec error nobody can act on.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::platform::{Error, Result};
use crate::soundtrack::{resample, Refusal, SAMPLE_RATE};

/// Extensions the decoder accepts, lower case.
pub const EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "ogg", "wav", "aiff", "aif"];

/// A decoded song at the game's rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audio {
    pub left: Vec<i16>,
    pub right: Vec<i16>,
    pub duration_ms: u64,
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.to_path_buf(), source }
}

fn bad(path: &Path, what: impl Into<String>) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, what.into()),
    }
}

/// Whether the extension is one the decoder reads.
pub fn is_supported(path: &Path) -> bool {
    extension_of(path).map(|e| EXTENSIONS.contains(&e.as_str())).unwrap_or(false)
}

fn extension_of(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase())
}

/// Decode a file to 24 kHz stereo.
pub fn decode_file(path: &Path) -> Result<Audio> {
    let extension = extension_of(path).unwrap_or_default();
    if !EXTENSIONS.contains(&extension.as_str()) {
        return Err(Refusal::UnsupportedFormat { extension: if extension.is_empty() { "(none)".into() } else { extension } }.into());
    }
    let file = File::open(path).map_err(|e| io(path, e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(&extension);
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| bad(path, format!("not a readable audio file: {e}")))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| bad(path, "no audio track"))?;
    let track_id = track.id;
    let params = track.codec_params.clone();
    let rate = params.sample_rate.ok_or_else(|| bad(path, "the audio track does not state its sample rate"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .map_err(|e| bad(path, format!("unsupported codec: {e}")))?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => {
                decoder = symphonia::default::get_codecs()
                    .make(&params, &DecoderOptions::default())
                    .map_err(|e| bad(path, format!("unsupported codec: {e}")))?;
                continue;
            }
            Err(e) => return Err(bad(path, format!("decode failed: {e}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue, // a damaged frame, skip it
            Err(SymError::ResetRequired) => {
                decoder = symphonia::default::get_codecs()
                    .make(&params, &DecoderOptions::default())
                    .map_err(|e| bad(path, format!("unsupported codec: {e}")))?;
                continue;
            }
            Err(e) => return Err(bad(path, format!("decode failed: {e}"))),
        };
        let spec = *decoded.spec();
        let n_channels = spec.channels.count();
        if channels.len() != n_channels {
            channels = (0..n_channels).map(|_| Vec::new()).collect();
        }
        let frames = decoded.capacity() as u64;
        let buf = match &mut sample_buf {
            Some(b) if b.capacity() >= decoded.capacity() * n_channels => b,
            _ => sample_buf.insert(SampleBuffer::<f32>::new(frames, spec)),
        };
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();
        for (i, s) in samples.iter().enumerate() {
            channels[i % n_channels].push(*s);
        }
    }
    if channels.is_empty() || channels[0].is_empty() {
        return Err(bad(path, "the file decoded to no audio"));
    }
    let (left, right) = resample::to_24k(&channels, rate);
    let duration_ms = left.len() as u64 * 1000 / u64::from(SAMPLE_RATE);
    Ok(Audio { left, right, duration_ms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundtrack::testing::{write_wav, write_wav_24k};

    fn tone(n: usize, rate: f64) -> Vec<i16> {
        (0..n).map(|i| ((2.0 * std::f64::consts::PI * 440.0 * i as f64 / rate).sin() * 12000.0).round() as i16).collect()
    }

    #[test]
    fn a_24k_wav_decodes_to_the_same_samples() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let left = tone(24_000, 24_000.0);
        let right: Vec<i16> = left.iter().map(|s| s / 2).collect();
        write_wav_24k(&path, &left, &right);
        let a = decode_file(&path).unwrap();
        assert_eq!(a.left.len(), left.len());
        assert_eq!(a.duration_ms, 1000);
        // symphonia's PCM path hands back the exact 16-bit values as f32/32768,
        // which round-trips within one step through the f32 scale
        let max_err = a.left.iter().zip(&left).map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs()).max().unwrap();
        assert!(max_err <= 1, "max error {max_err}");
        let max_err_r = a.right.iter().zip(&right).map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs()).max().unwrap();
        assert!(max_err_r <= 1, "max error R {max_err_r}");
    }

    #[test]
    fn a_48k_wav_decodes_to_half_the_samples() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone48.wav");
        let left = tone(48_001, 48_000.0);
        write_wav(&path, 48_000, &left, &left);
        let a = decode_file(&path).unwrap();
        assert!((a.left.len() as i64 - 24_000).abs() <= 1, "{}", a.left.len());
        assert_eq!(a.left.len(), a.right.len());
        assert_eq!(a.duration_ms, 1000);
    }

    #[test]
    fn an_unsupported_extension_is_refused_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"hello").unwrap();
        let err = decode_file(&path).unwrap_err().to_string();
        assert!(err.contains(".txt") && err.contains("MP3"), "{err}");
        assert!(!is_supported(&path));
        assert!(is_supported(Path::new("x/Song.MP3")));
        let garbage = dir.path().join("garbage.mp3");
        std::fs::write(&garbage, b"this is not audio at all").unwrap();
        assert!(decode_file(&garbage).is_err());
    }

    /// A real MP3 from the user's Music folder, when one exists.
    #[test]
    fn a_real_mp3_from_the_music_folder_decodes() {
        fn find_mp3(dir: &Path, depth: usize) -> Option<std::path::PathBuf> {
            if depth > 4 {
                return None;
            }
            let entries = std::fs::read_dir(dir).ok()?;
            let mut dirs = Vec::new();
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file() && extension_of(&p).as_deref() == Some("mp3") {
                    return Some(p);
                }
                if p.is_dir() {
                    dirs.push(p);
                }
            }
            dirs.into_iter().find_map(|d| find_mp3(&d, depth + 1))
        }
        let Some(home) = std::env::var_os("USERPROFILE") else {
            eprintln!("skipped: no mp3 in Music");
            return;
        };
        let Some(mp3) = find_mp3(&Path::new(&home).join("Music"), 0) else {
            eprintln!("skipped: no mp3 in Music");
            return;
        };
        let a = decode_file(&mp3).unwrap();
        assert!(a.duration_ms > 0);
        assert_eq!(a.left.len(), a.right.len());
        let tags = crate::soundtrack::tags::read(&mp3);
        eprintln!("decoded {} ({} ms), tags {:?}", mp3.display(), a.duration_ms, tags);
    }
}
