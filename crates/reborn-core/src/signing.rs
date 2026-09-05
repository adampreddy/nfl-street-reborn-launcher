//! Ed25519 over the release manifest, exactly the bytes served.
//!
//! `CONTENT_AND_UPDATE_MODEL.md` section 6 fixes the shape: a detached
//! Ed25519 signature over the canonical bytes of `release.json`, the public key
//! compiled into the launcher, and a second offline-held public key compiled in
//! beside it so a rotation does not force every player to reinstall. This
//! module is that, and nothing more: it does not parse the manifest, does not
//! fetch anything, and does not decide what a verified release means. That is
//! [`crate::content`]'s job, and it takes an [`Ed25519Verifier`] through the
//! [`SignatureVerifier`] trait.
//!
//! # The empty trust anchor is an error, never a pass
//!
//! [`TRUSTED_KEYS`] is empty until Adam generates the publisher key. A build
//! with no key compiled in cannot verify anything, and [`SignatureVerifier`]
//! draws the line that matters here: an `Err` means the check could not be
//! performed, `Ok(false)` means it was performed and failed. Nothing in this
//! file can produce `Ok(true)` without a key that actually verified the bytes,
//! and there is deliberately no constructor that accepts everything. The
//! content module's own tests keep a fixed-signature double for that, private
//! to the test module, for exactly the reason its comment gives.
//!
//! # On the wire
//!
//! The signature travels as a text file beside the manifest, `release.json.sig`,
//! holding 128 lowercase hex characters. Text rather than 64 raw bytes because
//! it survives every CDN, editor, clipboard and mail client unchanged, and a
//! signature that gets a stray byte appended is a failed verification with no
//! clue as to why. [`decode_signature`] tolerates surrounding whitespace and
//! nothing else.

use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};

use crate::content::SignatureVerifier;
use crate::{Error, Result};

/// Length of a raw Ed25519 public key.
pub const PUBLIC_KEY_LEN: usize = 32;
/// Length of a raw Ed25519 signature.
pub const SIGNATURE_LEN: usize = 64;

/// The publisher keys this build trusts, in the order they are tried.
///
/// **Empty until the key exists.** The first entry is the everyday publisher
/// key. The second is the rotation key, generated at the same time, whose
/// secret half is written down and put somewhere that is not a computer; it is
/// never used unless the first is lost or leaked. Adding a key here is a
/// release of the launcher, which is the point: the launcher is the one thing
/// a player has to obtain out of band anyway, so it is where trust is rooted.
///
/// Filled in by `rebornctl publish keygen`, which prints exactly the lines to
/// paste here.
pub const TRUSTED_KEYS: &[[u8; PUBLIC_KEY_LEN]] = &[];

/// Verifies release manifests against a fixed set of public keys.
#[derive(Debug, Clone)]
pub struct Ed25519Verifier {
    keys: Vec<VerifyingKey>,
}

impl Ed25519Verifier {
    /// The verifier for the keys compiled into this build. May hold no keys,
    /// in which case every verification is an error rather than a result; see
    /// [`Ed25519Verifier::has_trust_anchor`].
    pub fn embedded() -> Self {
        Self::with_keys(TRUSTED_KEYS).expect("the compiled-in keys are valid Ed25519 points")
    }

    /// A verifier for the given raw public keys.
    ///
    /// Refuses a byte string that is not a valid Ed25519 point, because a key
    /// that can never verify anything, silently skipped, would leave a build
    /// that looks signed and trusts nothing.
    pub fn with_keys(raw: &[[u8; PUBLIC_KEY_LEN]]) -> Result<Self> {
        let mut keys = Vec::with_capacity(raw.len());
        for (i, bytes) in raw.iter().enumerate() {
            let key = VerifyingKey::from_bytes(bytes).map_err(|e| {
                invalid(format!(
                    "trusted key {i} is not a valid Ed25519 public key: {e}. \
                     It was pasted wrong; run keygen again and paste all 32 bytes"
                ))
            })?;
            keys.push(key);
        }
        Ok(Self { keys })
    }

    /// Whether this verifier can verify anything at all.
    pub fn has_trust_anchor(&self) -> bool {
        !self.keys.is_empty()
    }

    /// How many keys are trusted, for diagnostics.
    pub fn key_count(&self) -> usize {
        self.keys.len()
    }
}

impl SignatureVerifier for Ed25519Verifier {
    fn verify(&self, signed_bytes: &[u8], signature: &[u8]) -> Result<bool> {
        if self.keys.is_empty() {
            return Err(invalid(
                "no publisher key is compiled into this build, so a release \
                 signature cannot be checked. Nothing was downloaded and nothing \
                 was changed. This launcher can play but cannot update until a \
                 build with the key ships",
            ));
        }
        // A signature of the wrong length is a signature that fails, not a
        // check that could not be performed: the file was there, it was read,
        // and it is not a signature over these bytes.
        let Ok(sig) = Signature::from_slice(signature) else {
            return Ok(false);
        };
        Ok(self.keys.iter().any(|k| k.verify(signed_bytes, &sig).is_ok()))
    }
}

/// Encode a raw signature the way it is published: 128 lowercase hex characters.
pub fn encode_signature(sig: &[u8; SIGNATURE_LEN]) -> String {
    hex::encode(sig)
}

/// Decode a published signature. Surrounding whitespace is tolerated because a
/// trailing newline is what every text tool adds; anything else is `None`.
pub fn decode_signature(text: &str) -> Option<[u8; SIGNATURE_LEN]> {
    let bytes = hex::decode(text.trim()).ok()?;
    bytes.try_into().ok()
}

/// Encode a public key for pasting into [`TRUSTED_KEYS`].
pub fn encode_public_key(key: &[u8; PUBLIC_KEY_LEN]) -> String {
    hex::encode(key)
}

/// Decode a public key from its hex form.
pub fn decode_public_key(text: &str) -> Option<[u8; PUBLIC_KEY_LEN]> {
    let bytes = hex::decode(text.trim()).ok()?;
    bytes.try_into().ok()
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, msg.into()))
}

/// The publisher's half. Key generation and signing, compiled only into the
/// tool that publishes, never into a player's launcher.
#[cfg(feature = "publish")]
pub mod publisher {
    use ed25519_dalek::{Signer as _, SigningKey};

    use super::{invalid, PUBLIC_KEY_LEN, SIGNATURE_LEN};
    use crate::Result;

    /// Length of a raw Ed25519 secret key.
    pub const SECRET_KEY_LEN: usize = 32;

    /// A freshly generated key pair.
    #[derive(Debug)]
    pub struct KeyPair {
        pub secret: [u8; SECRET_KEY_LEN],
        pub public: [u8; PUBLIC_KEY_LEN],
    }

    /// Generate a key pair from the operating system's random source.
    ///
    /// Two are generated at publish setup: the everyday key and the rotation
    /// key. Both public halves go into [`super::TRUSTED_KEYS`]; only the first
    /// secret stays on a machine.
    pub fn generate() -> Result<KeyPair> {
        let mut secret = [0u8; SECRET_KEY_LEN];
        getrandom::getrandom(&mut secret)
            .map_err(|e| invalid(format!("the operating system gave no random bytes: {e}")))?;
        let signing = SigningKey::from_bytes(&secret);
        Ok(KeyPair { secret, public: signing.verifying_key().to_bytes() })
    }

    /// The public half of a secret key, for checking which key a file holds.
    pub fn public_of(secret: &[u8; SECRET_KEY_LEN]) -> [u8; PUBLIC_KEY_LEN] {
        SigningKey::from_bytes(secret).verifying_key().to_bytes()
    }

    /// Sign exactly these bytes. The caller signs the file as it will be
    /// served, never a re-serialized copy of it.
    pub fn sign(secret: &[u8; SECRET_KEY_LEN], bytes: &[u8]) -> [u8; SIGNATURE_LEN] {
        SigningKey::from_bytes(secret).sign(bytes).to_bytes()
    }

    /// Encode a secret key for the key file. Hex, like everything else here.
    pub fn encode_secret(secret: &[u8; SECRET_KEY_LEN]) -> String {
        hex::encode(secret)
    }

    /// Decode a secret key from the key file.
    pub fn decode_secret(text: &str) -> Option<[u8; SECRET_KEY_LEN]> {
        let bytes = hex::decode(text.trim()).ok()?;
        bytes.try_into().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};

    // Keys from fixed seeds, straight from the curve library, so these tests
    // compile and run in the default build. The `publish` feature only adds a
    // random source for generating real keys; the arithmetic is the same.
    struct Pair {
        secret: [u8; 32],
        public: [u8; PUBLIC_KEY_LEN],
    }

    fn pair_from(seed: u8) -> Pair {
        let secret = [seed; 32];
        Pair { secret, public: SigningKey::from_bytes(&secret).verifying_key().to_bytes() }
    }

    fn sign(secret: &[u8; 32], bytes: &[u8]) -> [u8; SIGNATURE_LEN] {
        SigningKey::from_bytes(secret).sign(bytes).to_bytes()
    }

    mod publisher {
        //! The same three calls the real publisher module offers, over fixed
        //! seeds, so the tests below read the same whether or not the feature
        //! is on.
        use super::*;
        pub struct KeyPair {
            pub secret: [u8; 32],
            pub public: [u8; PUBLIC_KEY_LEN],
        }
        pub fn sign(secret: &[u8; 32], bytes: &[u8]) -> [u8; SIGNATURE_LEN] {
            super::sign(secret, bytes)
        }
        pub fn public_of(secret: &[u8; 32]) -> [u8; PUBLIC_KEY_LEN] {
            SigningKey::from_bytes(secret).verifying_key().to_bytes()
        }
        pub fn encode_secret(secret: &[u8; 32]) -> String {
            hex::encode(secret)
        }
        pub fn decode_secret(text: &str) -> Option<[u8; 32]> {
            hex::decode(text.trim()).ok()?.try_into().ok()
        }
    }

    static NEXT_SEED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

    fn pair() -> publisher::KeyPair {
        let seed = NEXT_SEED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = pair_from(seed);
        publisher::KeyPair { secret: p.secret, public: p.public }
    }

    #[test]
    fn a_signature_by_a_trusted_key_verifies_and_one_by_a_stranger_does_not() {
        let ours = pair();
        let theirs = pair();
        let bytes = br#"{"schema":"nsr.release.v1","version":"2.4.0","sequence":7,"components":{}}"#;

        let verifier = Ed25519Verifier::with_keys(&[ours.public]).expect("valid key");
        let good = publisher::sign(&ours.secret, bytes);
        let forged = publisher::sign(&theirs.secret, bytes);

        assert_eq!(verifier.verify(bytes, &good).expect("check ran"), true);
        assert_eq!(verifier.verify(bytes, &forged).expect("check ran"), false);
    }

    #[test]
    fn one_changed_byte_in_the_manifest_fails_the_signature() {
        let ours = pair();
        let bytes = b"{\"schema\":\"nsr.release.v1\",\"version\":\"2.4.0\",\"sequence\":7,\"components\":{}}";
        let sig = publisher::sign(&ours.secret, bytes);
        let verifier = Ed25519Verifier::with_keys(&[ours.public]).expect("valid key");

        let mut altered = bytes.to_vec();
        // sequence 7 becomes sequence 8: the replay attack section 4 of the
        // design exists to stop, in one byte.
        let at = altered.iter().position(|&b| b == b'7').expect("a 7");
        altered[at] = b'8';

        assert!(verifier.verify(bytes, &sig).expect("check ran"));
        assert!(!verifier.verify(&altered, &sig).expect("check ran"));
    }

    #[test]
    fn the_rotation_key_verifies_too_so_a_rotation_needs_no_reinstall() {
        let everyday = pair();
        let rotation = pair();
        let bytes = b"release";
        let verifier =
            Ed25519Verifier::with_keys(&[everyday.public, rotation.public]).expect("valid keys");

        let by_rotation = publisher::sign(&rotation.secret, bytes);
        assert!(verifier.verify(bytes, &by_rotation).expect("check ran"));
    }

    #[test]
    fn no_compiled_in_key_is_an_error_not_a_failed_check() {
        let verifier = Ed25519Verifier::with_keys(&[]).expect("empty is allowed");
        assert!(!verifier.has_trust_anchor());
        let outcome = verifier.verify(b"anything", &[0u8; SIGNATURE_LEN]);
        assert!(outcome.is_err(), "an unperformable check must not read as a failed one");
        let msg = outcome.unwrap_err().to_string();
        assert!(msg.contains("cannot update"), "says what still works: {msg}");
        assert!(msg.contains("nothing was changed"), "says what remains safe: {msg}");
    }

    #[test]
    fn the_build_as_shipped_has_no_trust_anchor_yet_and_says_so() {
        // This test will need updating the day the key lands, which is the
        // point: it makes that day a deliberate edit rather than a surprise.
        let verifier = Ed25519Verifier::embedded();
        assert_eq!(verifier.key_count(), TRUSTED_KEYS.len());
        assert!(!verifier.has_trust_anchor(), "TRUSTED_KEYS is still empty");
    }

    #[test]
    fn a_malformed_signature_is_a_failed_check_not_an_error() {
        let ours = pair();
        let verifier = Ed25519Verifier::with_keys(&[ours.public]).expect("valid key");
        assert_eq!(verifier.verify(b"x", b"too short").expect("check ran"), false);
        assert_eq!(verifier.verify(b"x", &[0u8; 65]).expect("check ran"), false);
    }

    #[test]
    fn signatures_and_keys_round_trip_through_their_text_form() {
        let ours = pair();
        let sig = publisher::sign(&ours.secret, b"bytes");

        let text = encode_signature(&sig);
        assert_eq!(text.len(), 128);
        assert_eq!(decode_signature(&text), Some(sig));
        assert_eq!(decode_signature(&format!("  {text}\r\n")), Some(sig), "a newline is not damage");
        assert_eq!(decode_signature(&text[..127]), None, "a dropped character is");
        assert_eq!(decode_signature(&format!("{text}0")), None, "and so is an added one");

        let key_text = encode_public_key(&ours.public);
        assert_eq!(decode_public_key(&key_text), Some(ours.public));
        assert_eq!(publisher::decode_secret(&publisher::encode_secret(&ours.secret)), Some(ours.secret));
        assert_eq!(publisher::public_of(&ours.secret), ours.public);
    }

    #[test]
    fn a_pasted_key_that_is_not_a_point_is_refused_at_construction() {
        // Not every 32 byte string is a curve point: about half of all y
        // coordinates have no x on the curve. Which ones is a fact about the
        // curve, not something to hard-code (a first draft assumed all-0xff
        // and was wrong), so ask the library for the first repeated-byte
        // encoding it rejects and use that.
        let not_a_point = (0u8..=255)
            .map(|b| [b; PUBLIC_KEY_LEN])
            .find(|bytes| VerifyingKey::from_bytes(bytes).is_err())
            .expect("some repeated-byte encoding is off the curve");
        let outcome = Ed25519Verifier::with_keys(&[not_a_point]);
        assert!(outcome.is_err(), "{not_a_point:?} must be refused");
        assert!(outcome.unwrap_err().to_string().contains("not a valid Ed25519 public key"));
    }
}
