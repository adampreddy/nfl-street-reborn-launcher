//! Signed release and component manifests.
//!
//! # The shape, and why it is this one
//!
//! Two schemas, one signature.
//!
//! ```text
//!   nsr.release.v1        <- the ONLY signed object
//!     pins each component by SHA-256
//!         |
//!         +-- nsr.component.v1   runtime
//!         +-- nsr.component.v1   offline-content
//!         +-- nsr.component.v1   online-content
//!         +-- ...
//! ```
//!
//! Component manifests are never individually signed. Pinning them by hash from
//! one signed release means a component manifest cannot be swapped without
//! breaking the release signature, and it means one signature per release
//! rather than seven.
//!
//! Two earlier drafts of the architecture specified incompatible designs, one
//! per-component and one release-level, under two different schema names. This
//! is the resolved one.
//!
//! # The field that stops repair deleting your music
//!
//! [`Component::customizable`] is mandatory and is true for `offline-content`
//! only. A rebuilt offline image never matches its shipped hash, so a plain
//! "hash mismatch means damage" rule would treat every user's custom soundtrack
//! as corruption and re-fetch over it. See [`crate::library`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A hex-encoded SHA-256 digest.
///
/// A newtype rather than a `String` because the codebase passes several kinds
/// of hex around and mixing a file hash with a manifest hash would be silent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sha256Hex(String);

impl Sha256Hex {
    /// Parse, rejecting anything that is not 64 lowercase hex characters.
    pub fn parse(s: &str) -> Result<Self, ManifestError> {
        if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ManifestError::BadDigest(s.to_string()));
        }
        Ok(Self(s.to_ascii_lowercase()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(bytes);
        Self(hex::encode(h.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Sha256Hex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("not a sha-256 digest: {0:?}")]
    BadDigest(String),

    #[error("unsupported schema {found:?}, expected {expected:?}")]
    WrongSchema {
        found: String,
        expected: &'static str,
    },

    #[error(
        "manifest is for component {found:?} but {requested:?} was requested. \
         Refusing: a component manifest served under the wrong name is either a \
         mistake or an attack"
    )]
    WrongComponent { found: String, requested: String },

    #[error(
        "manifest install_root is {found:?} but {expected:?} was expected. \
         Refusing: this decides where files land on disk"
    )]
    WrongInstallRoot { found: String, expected: String },

    #[error(
        "sequence {found} is not newer than the installed sequence {installed}. \
         Refusing: accepting it would allow a rollback to a known-bad release"
    )]
    StaleSequence { found: u64, installed: u64 },

    #[error("release does not pin a component named {0:?}")]
    UnpinnedComponent(String),

    #[error(
        "component manifest for {component:?} hashes to {actual} but the signed \
         release pins {expected}"
    )]
    ComponentHashMismatch {
        component: String,
        expected: Sha256Hex,
        actual: Sha256Hex,
    },

    #[error("component {0:?} does not declare the mandatory customizable field")]
    MissingCustomizable(String),

    #[error(
        "component {component:?} declares members_only = {declared} but that \
         component is members_only = {expected}"
    )]
    MembersOnlyMismatch {
        component: String,
        declared: bool,
        expected: bool,
    },

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// The one signed object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    /// Must be exactly `nsr.release.v1`.
    pub schema: String,

    /// Human-facing release version, for example `2.4.0`.
    pub version: String,

    /// Monotonically increasing. Mandatory, and the client refuses anything not
    /// strictly newer than what is installed.
    ///
    /// Without this, an attacker who can serve files could replay an older
    /// signed release to reintroduce a fixed vulnerability. The signature would
    /// still verify, because it was genuinely signed once.
    pub sequence: u64,

    /// Component name to its pinned manifest.
    pub components: BTreeMap<String, PinnedComponent>,

    /// What changed, in the publisher's words, shown on the update card.
    /// Signed with everything else. Optional both ways: a launcher that
    /// predates it ignores the field, and a release without one is shown as
    /// the list of components it carries, as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// A component manifest pinned by hash from the signed release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedComponent {
    /// Where to fetch the component manifest.
    pub url: String,
    /// SHA-256 of the component manifest bytes, exactly as served.
    pub sha256: Sha256Hex,
}

pub const RELEASE_SCHEMA: &str = "nsr.release.v1";
pub const COMPONENT_SCHEMA: &str = "nsr.component.v1";

impl Release {
    /// Parse and validate, without checking the signature.
    ///
    /// Signature verification happens before this, on the exact bytes served.
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let release: Release = serde_json::from_slice(bytes)?;
        if release.schema != RELEASE_SCHEMA {
            return Err(ManifestError::WrongSchema {
                found: release.schema,
                expected: RELEASE_SCHEMA,
            });
        }
        Ok(release)
    }

    /// Reject a release that is not strictly newer than what is installed.
    pub fn check_sequence(&self, installed: u64) -> Result<(), ManifestError> {
        if self.sequence <= installed {
            return Err(ManifestError::StaleSequence {
                found: self.sequence,
                installed,
            });
        }
        Ok(())
    }

    /// Validate a component manifest against this release's pin.
    pub fn accept_component(
        &self,
        requested: &str,
        bytes: &[u8],
    ) -> Result<Component, ManifestError> {
        let pin = self
            .components
            .get(requested)
            .ok_or_else(|| ManifestError::UnpinnedComponent(requested.to_string()))?;

        let actual = Sha256Hex::from_bytes(bytes);
        if actual != pin.sha256 {
            return Err(ManifestError::ComponentHashMismatch {
                component: requested.to_string(),
                expected: pin.sha256.clone(),
                actual,
            });
        }

        let component = Component::parse(bytes)?;
        if component.component != requested {
            return Err(ManifestError::WrongComponent {
                found: component.component,
                requested: requested.to_string(),
            });
        }
        // The flag is derived from the identity, so a manifest that
        // disagrees is wrong or hostile either way.
        if let Some(id) = crate::inventory::ComponentId::parse(requested) {
            if component.members_only != id.is_members_only() {
                return Err(ManifestError::MembersOnlyMismatch {
                    component: requested.to_string(),
                    declared: component.members_only,
                    expected: id.is_members_only(),
                });
            }
        }
        Ok(component)
    }
}

/// One updateable component. Never signed on its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    /// Must be exactly `nsr.component.v1`.
    pub schema: String,

    /// Component name. Mandatory, and checked against what was requested.
    pub component: String,

    pub version: String,

    /// Mandatory here too. A component manifest can be updated between
    /// releases, so it carries its own replay protection.
    pub sequence: u64,

    /// Which data directory the files land in, as a relative path.
    ///
    /// Mandatory, and the client refuses a manifest whose root differs from the
    /// one it expected. This is what stops a manifest for `runtime` writing
    /// into `saves`.
    pub install_root: String,

    /// Whether the user is allowed to modify these files locally.
    ///
    /// **Mandatory.** True for `offline-content` only. Repair consults this
    /// before treating a hash mismatch as damage, which is what keeps a rebuilt
    /// soundtrack from being re-fetched over. See [`crate::library::Repairer`].
    pub customizable: bool,

    /// Whether this component's blobs are withheld from the public CDN.
    ///
    /// True for `league-game` and `league-content` only, derived from the
    /// identity like `customizable`. The manifest itself is public and pinned
    /// like any other. A launcher fetches the blobs through the backend's
    /// `POST /v1/league/download` (signed, short-lived URLs), never from
    /// `<origin>/blobs/by-hash`. Absent means false, and false is never
    /// written, so every manifest published before this field existed still
    /// round-trips byte for byte.
    #[serde(default, skip_serializing_if = "is_false")]
    pub members_only: bool,

    pub files: Vec<ManifestFile>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestFile {
    /// Relative to the component's `install_root`. Must not escape it.
    pub path: String,
    pub bytes: u64,
    pub sha256: Sha256Hex,
}

impl Component {
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let c: Component = serde_json::from_slice(bytes)?;
        if c.schema != COMPONENT_SCHEMA {
            return Err(ManifestError::WrongSchema {
                found: c.schema,
                expected: COMPONENT_SCHEMA,
            });
        }
        Ok(c)
    }

    /// Reject a manifest whose install root is not the expected one.
    pub fn check_install_root(&self, expected: &str) -> Result<(), ManifestError> {
        if self.install_root != expected {
            return Err(ManifestError::WrongInstallRoot {
                found: self.install_root.clone(),
                expected: expected.to_string(),
            });
        }
        Ok(())
    }

    /// Total size, for the progress display and the disk space check.
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }

    /// Paths that try to escape the install root.
    ///
    /// A package is refused outright if this is non-empty. `..`, an absolute
    /// path, or a Windows drive letter in a manifest is not a mistake worth
    /// tolerating.
    pub fn traversal_offenders(&self) -> Vec<&str> {
        self.files
            .iter()
            .map(|f| f.path.as_str())
            .filter(|p| {
                p.starts_with('/')
                    || p.starts_with('\\')
                    || p.contains(':')
                    || p.split(['/', '\\']).any(|seg| seg == "..")
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component_json(name: &str, customizable: bool) -> String {
        format!(
            r#"{{
              "schema": "nsr.component.v1",
              "component": "{name}",
              "version": "2.4.0",
              "sequence": 7,
              "install_root": "library/offline",
              "customizable": {customizable},
              "files": [
                {{"path": "NSR.iso", "bytes": 1459978240,
                  "sha256": "78a987c7780071a84a4d89fa4cfac62e717551fcdc3c2539facbefed970d345e"}}
              ]
            }}"#
        )
    }

    #[test]
    fn digest_parsing_rejects_anything_that_is_not_64_hex() {
        assert!(Sha256Hex::parse(&"a".repeat(64)).is_ok());
        assert!(Sha256Hex::parse(&"A".repeat(64)).is_ok(), "case normalized");
        assert!(Sha256Hex::parse(&"a".repeat(63)).is_err());
        assert!(Sha256Hex::parse(&"a".repeat(65)).is_err());
        assert!(Sha256Hex::parse(&"z".repeat(64)).is_err());
        assert!(Sha256Hex::parse("").is_err());
    }

    #[test]
    fn digest_of_known_input_matches_the_reference_value() {
        // The SHA-256 of the empty string, so this test also proves the hash
        // wiring is the real algorithm and not a placeholder.
        assert_eq!(
            Sha256Hex::from_bytes(b"").as_str(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn a_component_manifest_must_declare_its_schema() {
        let bad = component_json("offline-content", true).replace("nsr.component.v1", "something");
        let err = Component::parse(bad.as_bytes()).unwrap_err();
        assert!(matches!(err, ManifestError::WrongSchema { .. }), "{err:?}");
    }

    /// `customizable` is mandatory. Serde has no default for it deliberately,
    /// so a manifest that omits it fails to parse rather than defaulting to
    /// false, which would make repair delete custom music.
    #[test]
    fn customizable_is_mandatory_and_has_no_default() {
        let without = r#"{
          "schema": "nsr.component.v1", "component": "offline-content",
          "version": "2.4.0", "sequence": 7, "install_root": "library/offline",
          "files": []
        }"#;
        let err = Component::parse(without.as_bytes()).unwrap_err();
        assert!(
            matches!(err, ManifestError::Json(_)),
            "omitting customizable must fail to parse, got {err:?}"
        );
    }

    #[test]
    fn install_root_mismatch_is_refused() {
        let c = Component::parse(component_json("offline-content", true).as_bytes()).unwrap();
        assert!(c.check_install_root("library/offline").is_ok());
        let err = c.check_install_root("saves").unwrap_err();
        assert!(
            matches!(err, ManifestError::WrongInstallRoot { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn path_traversal_is_detected() {
        let mut c = Component::parse(component_json("offline-content", true).as_bytes()).unwrap();
        assert!(c.traversal_offenders().is_empty());

        for evil in [
            "../../saves/MemoryCardA.USA.raw",
            "/etc/passwd",
            "C:\\Windows\\System32\\evil.dll",
            "textures\\..\\..\\..\\boot.ini",
        ] {
            c.files.push(ManifestFile {
                path: evil.to_string(),
                bytes: 1,
                sha256: Sha256Hex::from_bytes(b""),
            });
        }
        assert_eq!(
            c.traversal_offenders().len(),
            4,
            "{:?}",
            c.traversal_offenders()
        );
    }

    #[test]
    fn a_release_pins_components_by_hash_and_rejects_a_swap() {
        let comp_bytes = component_json("offline-content", true).into_bytes();
        let pin = Sha256Hex::from_bytes(&comp_bytes);

        let release_json = format!(
            r#"{{
              "schema": "nsr.release.v1", "version": "2.4.0", "sequence": 12,
              "components": {{
                "offline-content": {{"url": "https://example/offline.json", "sha256": "{pin}"}}
              }}
            }}"#
        );
        let release = Release::parse(release_json.as_bytes()).unwrap();

        // The genuine manifest is accepted.
        let ok = release
            .accept_component("offline-content", &comp_bytes)
            .unwrap();
        assert!(ok.customizable);

        // One byte different, and it is refused.
        let mut tampered = comp_bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] = b' ';
        let err = release
            .accept_component("offline-content", &tampered)
            .unwrap_err();
        assert!(
            matches!(err, ManifestError::ComponentHashMismatch { .. }),
            "{err:?}"
        );
    }

    /// A manifest served under the wrong component name is refused even when
    /// its hash matches, because the hash only proves it was not altered in
    /// transit, not that it is the right object.
    #[test]
    fn a_manifest_for_the_wrong_component_is_refused() {
        let comp_bytes = component_json("runtime", false).into_bytes();
        let pin = Sha256Hex::from_bytes(&comp_bytes);
        let release_json = format!(
            r#"{{
              "schema": "nsr.release.v1", "version": "2.4.0", "sequence": 12,
              "components": {{
                "offline-content": {{"url": "https://e/x.json", "sha256": "{pin}"}}
              }}
            }}"#
        );
        let release = Release::parse(release_json.as_bytes()).unwrap();
        let err = release
            .accept_component("offline-content", &comp_bytes)
            .unwrap_err();
        assert!(
            matches!(err, ManifestError::WrongComponent { .. }),
            "{err:?}"
        );
    }

    /// Replay protection. A genuinely signed older release must still be
    /// refused, or a fixed vulnerability can be reintroduced by serving an old
    /// file.
    #[test]
    fn an_older_sequence_is_refused_even_though_it_was_validly_signed() {
        let release_json = r#"{"schema":"nsr.release.v1","version":"2.3.0",
                               "sequence": 11, "components": {}}"#;
        let release = Release::parse(release_json.as_bytes()).unwrap();
        assert!(release.check_sequence(10).is_ok(), "11 is newer than 10");
        assert!(release.check_sequence(11).is_err(), "equal is not newer");
        let err = release.check_sequence(12).unwrap_err();
        assert!(
            matches!(err, ManifestError::StaleSequence { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn total_bytes_sums_the_file_list() {
        let c = Component::parse(component_json("offline-content", true).as_bytes()).unwrap();
        assert_eq!(c.total_bytes(), 1_459_978_240);
    }

    /// Every manifest published before league content existed has no
    /// `members_only` key. It must still parse, as public.
    #[test]
    fn a_manifest_without_members_only_parses_as_public() {
        let c = Component::parse(component_json("offline-content", true).as_bytes()).unwrap();
        assert!(!c.members_only);
    }

    /// And it must serialize back without the key, byte for byte, or the
    /// release signer's round-trip check refuses every old manifest.
    #[test]
    fn a_public_manifest_serializes_without_the_members_only_key() {
        let c = Component::parse(component_json("runtime", false).as_bytes()).unwrap();
        let out = serde_json::to_string_pretty(&c).unwrap();
        assert!(!out.contains("members_only"), "{out}");
    }

    #[test]
    fn a_members_only_manifest_says_so_and_round_trips() {
        let json = component_json("league-game", false)
            .replace("\"customizable\": false,", "\"customizable\": false,
  \"members_only\": true,");
        let c = Component::parse(json.as_bytes()).unwrap();
        assert!(c.members_only);
        let out = serde_json::to_string_pretty(&c).unwrap();
        assert!(out.contains("\"members_only\": true"), "{out}");
    }

    fn release_pinning(name: &str, json: &str) -> Release {
        let pin = Sha256Hex::from_bytes(json.as_bytes());
        Release::parse(
            format!(
                r#"{{"schema": "nsr.release.v1", "version": "2.4.0", "sequence": 12,
                  "components": {{"{name}": {{"url": "https://example/x.json", "sha256": "{pin}"}}}}}}"#
            )
            .as_bytes(),
        )
        .unwrap()
    }

    fn with_members_only(json: String) -> String {
        json.replace("\"customizable\": false,", "\"customizable\": false,
  \"members_only\": true,")
    }

    #[test]
    fn a_league_manifest_marked_public_is_refused_on_load() {
        let json = component_json("league-game", false);
        let err = release_pinning("league-game", &json)
            .accept_component("league-game", json.as_bytes())
            .unwrap_err();
        assert!(matches!(err, ManifestError::MembersOnlyMismatch { .. }), "{err:?}");
        assert!(!err.to_string().contains("  "), "no run of spaces: {err}");
    }

    #[test]
    fn a_retail_manifest_marked_members_only_is_refused_on_load() {
        let json = with_members_only(component_json("runtime", false));
        let err = release_pinning("runtime", &json)
            .accept_component("runtime", json.as_bytes())
            .unwrap_err();
        assert!(matches!(err, ManifestError::MembersOnlyMismatch { .. }), "{err:?}");
    }

    #[test]
    fn a_correctly_flagged_league_manifest_is_accepted_on_load() {
        let json = with_members_only(component_json("league-game", false));
        let c = release_pinning("league-game", &json)
            .accept_component("league-game", json.as_bytes())
            .unwrap();
        assert!(c.members_only);
    }
}
