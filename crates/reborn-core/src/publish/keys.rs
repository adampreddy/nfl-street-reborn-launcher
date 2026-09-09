//! The two keys, generated together once, and the key file format.

use std::path::{Path, PathBuf};

use crate::publish::{io_err, out, refuse, Outcome, Sink, PUBLISHER_KEY, ROTATION_KEY};
use crate::signing::{self, publisher, PUBLIC_KEY_LEN};
use crate::Result;

/// What keygen wrote. Public halves only; the secrets are on disk and nowhere
/// else.
#[derive(Debug, Clone)]
pub struct KeysWritten {
    pub publisher_path: PathBuf,
    pub rotation_path: PathBuf,
    pub everyday_public: [u8; PUBLIC_KEY_LEN],
    pub rotation_public: [u8; PUBLIC_KEY_LEN],
}

pub fn keygen(out_dir: &Path, sink: Sink<'_>) -> Result<Outcome<KeysWritten>> {
    let publisher_path = out_dir.join(PUBLISHER_KEY);
    let rotation_path = out_dir.join(ROTATION_KEY);
    for p in [&publisher_path, &rotation_path] {
        if p.exists() {
            return Ok(refuse(sink, &[
                format!("{} already exists. This tool never overwrites a key: a key that", p.display()),
                "is replaced by accident is a key that can no longer verify anything".to_string(),
                "already published. Move it aside deliberately if a new one is wanted.".to_string(),
            ]));
        }
    }
    std::fs::create_dir_all(out_dir).map_err(|e| io_err(out_dir, e))?;

    let everyday = publisher::generate()?;
    let rotation = publisher::generate()?;
    write_key(&publisher_path, &everyday.secret)?;
    write_key(&rotation_path, &rotation.secret)?;

    out!(sink, "Wrote {}", publisher_path.display());
    out!(sink, "Wrote {}", rotation_path.display());
    out!(sink);
    out!(sink, "Paste BOTH public keys into TRUSTED_KEYS in crates/reborn-core/src/signing.rs,");
    out!(sink, "everyday first. A launcher built with them will accept releases signed by");
    out!(sink, "either, which is what makes a rotation a launcher release and not a reinstall.");
    out!(sink);
    out!(sink, "    // everyday   {}", signing::encode_public_key(&everyday.public));
    out!(sink, "    {},", rust_array(&everyday.public));
    out!(sink, "    // rotation   {}", signing::encode_public_key(&rotation.public));
    out!(sink, "    {},", rust_array(&rotation.public));
    out!(sink);
    out!(sink, "Then:");
    out!(sink, "  {}  stays on the machine that publishes. Back it up. Never commit it.", PUBLISHER_KEY);
    out!(sink, "  {}   goes OFFLINE now: a USB stick in a drawer, or printed. It is only", ROTATION_KEY);
    out!(sink, "                 ever used if {} is lost or leaks, and a rotation key that", PUBLISHER_KEY);
    out!(sink, "                 sat beside the everyday key leaks in the same breach.");
    Ok(Outcome::Done(KeysWritten {
        publisher_path,
        rotation_path,
        everyday_public: everyday.public,
        rotation_public: rotation.public,
    }))
}

pub fn write_key(path: &Path, secret: &[u8; publisher::SECRET_KEY_LEN]) -> Result<()> {
    let text = format!("{}\n", publisher::encode_secret(secret));
    std::fs::write(path, text).map_err(|e| io_err(path, e))
}

pub fn rust_array(key: &[u8; PUBLIC_KEY_LEN]) -> String {
    let items: Vec<String> = key.iter().map(|b| format!("0x{b:02x}")).collect();
    format!("[{}]", items.join(", "))
}

pub fn read_key(path: &Path) -> Result<Option<[u8; publisher::SECRET_KEY_LEN]>> {
    let text = std::fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    Ok(publisher::decode_secret(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::{Event, Outcome};

    #[test]
    fn keygen_writes_both_keys_and_announces_both_public_halves() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("keys");
        let mut seen: Vec<Event> = Vec::new();
        let outcome = keygen(&out, &mut |e| seen.push(e)).unwrap();
        let written = match outcome {
            Outcome::Done(w) => w,
            other => panic!("expected Done, got {other:?}"),
        };
        assert!(written.publisher_path.is_file());
        assert!(written.rotation_path.is_file());
        // The secret on disk decodes and its public half is the announced one.
        let secret = read_key(&written.publisher_path).unwrap().expect("a key");
        assert_eq!(publisher::public_of(&secret), written.everyday_public);
        let text: Vec<String> = seen.iter().map(|e| e.line.clone()).collect();
        let everyday_hex = signing::encode_public_key(&written.everyday_public);
        assert!(text.iter().any(|l| l == &format!("    // everyday   {everyday_hex}")), "{text:?}");
        assert!(text.iter().any(|l| l.starts_with("Wrote ")));
        // The secret never appears in any line.
        let secret_hex = publisher::encode_secret(&secret);
        assert!(!text.iter().any(|l| l.contains(&secret_hex)));
    }

    #[test]
    fn keygen_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("keys");
        let mut seen: Vec<Event> = Vec::new();
        assert!(keygen(&out, &mut |e| seen.push(e)).unwrap().is_done());
        let before = std::fs::read(out.join(PUBLISHER_KEY)).unwrap();
        let outcome = keygen(&out, &mut |e| seen.push(e)).unwrap();
        assert!(outcome.refused().is_some(), "{outcome:?}");
        assert_eq!(std::fs::read(out.join(PUBLISHER_KEY)).unwrap(), before);
        assert!(seen.iter().any(|e| e.line.contains("already exists")));
    }

    #[test]
    fn read_key_returns_none_for_a_file_that_is_not_a_key() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("junk.key");
        std::fs::write(&p, "not hex\n").unwrap();
        assert!(read_key(&p).unwrap().is_none());
    }

    #[test]
    fn rust_array_formats_thirty_two_bytes() {
        let key = [0u8; PUBLIC_KEY_LEN];
        let s = rust_array(&key);
        assert!(s.starts_with("[0x00, 0x00"));
        assert_eq!(s.matches("0x").count(), PUBLIC_KEY_LEN);
    }
}
