//! The publisher's side of updates: turn a tree into an origin a launcher can
//! update from.
//!
//! Three verbs, in the order they are used once:
//!
//!   keygen     the publisher key and the rotation key, generated together
//!   component  hash one tree into the origin: its manifest and its blobs
//!   release    assemble release.json from components already in the origin,
//!              sign it, and prove the signed file verifies before saying so
//!
//! The origin is a directory. Every file it holds is served as-is by any
//! static host, which is the whole design (`origin.rs`): a bucket behind a
//! CDN, or `python -m http.server` on a laptop for the proof. Nothing here
//! talks to a network.
//!
//! # What is never done here
//!
//! A published file is never rewritten. A blob is content-addressed and so
//! immutable by construction; a component manifest that already exists with
//! different bytes is refused rather than replaced, because a client holding
//! the old release pins the old hash and a swapped file reads as tampering
//! on every machine that already has it. Bump the sequence and publish a new
//! version instead. `release.json` is the one file that changes, and only to
//! a strictly higher sequence.
//!
//! # The bytes are audited, never the ledger
//!
//! A scan already hashed every source file. The copy in the origin is hashed
//! again, because the copy is what gets served and a disk, a scanner or a
//! sync client can alter it between the two. A release refuses to pin a
//! component whose blobs are not all present at their published length, so
//! a release never names content the origin does not hold. And the signed
//! `release.json` is read back off the disk and verified with the public key
//! before the tool reports success, which is the only proof that what a
//! launcher will fetch is what was signed.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use reborn_core::content::TrustedRelease;
use reborn_core::inventory::{
    scan_component, ComponentId, ComponentSpec, ScanOutcome, SerializedComponent,
    UnsignedReleaseBuilder,
};
use reborn_core::library::hash_file;
use reborn_core::manifest::{Component, Release, Sha256Hex};
use reborn_core::origin::{blob_path, RELEASE_FILE, SIGNATURE_FILE};
use reborn_core::signing::{self, publisher, Ed25519Verifier};

use super::{misuse, thousands, ScanLog, OK, PROBLEM};

/// Where component manifests live under the origin.
const COMPONENTS_DIR: &str = "components";
/// The everyday key file name.
const PUBLISHER_KEY: &str = "publisher.key";
/// The rotation key file name. Generated with the first, then taken offline.
const ROTATION_KEY: &str = "rotation.key";

#[derive(Subcommand, Debug)]
pub enum PublishCommand {
    /// Generate the publisher key and the rotation key, together, once.
    Keygen {
        /// Directory to write publisher.key and rotation.key into. Refuses to
        /// overwrite either.
        #[arg(long)]
        out: PathBuf,
    },
    /// Hash one component tree into the origin: its manifest and its blobs.
    Component {
        /// The tree to publish. A pristine one: a mod file in it becomes
        /// content every client is told is genuine.
        #[arg(long)]
        root: PathBuf,
        /// launcher, runtime, offline-content, online-content, textures, mods,
        /// or controller-db.
        #[arg(long)]
        component: String,
        #[arg(long)]
        version: String,
        /// Strictly higher than any sequence this component has shipped with.
        #[arg(long)]
        sequence: u64,
        /// Required for textures: library/textures/<game id>.
        #[arg(long)]
        install_root: Option<String>,
        /// The origin directory. Created if absent.
        #[arg(long)]
        origin: PathBuf,
        /// Write. Without this the tree is hashed and the work is described.
        #[arg(long)]
        apply: bool,
    },
    /// Assemble release.json from components already in the origin, and sign it.
    Release {
        #[arg(long)]
        origin: PathBuf,
        /// The release label players see, for example 2.4.0.
        #[arg(long)]
        version: String,
        /// Strictly higher than the sequence of the release.json already in
        /// the origin, when there is one.
        #[arg(long)]
        sequence: u64,
        /// The publisher.key file from keygen.
        #[arg(long)]
        key: PathBuf,
        /// A component to pin, as name=version, once per component. Its
        /// manifest must already be in the origin from `publish component`.
        #[arg(long = "component", value_name = "NAME=VERSION")]
        components: Vec<String>,
        /// Write and sign. Without this everything is checked and nothing is
        /// written.
        #[arg(long)]
        apply: bool,
    },
    /// Fetch the roster the site published and push it: the component, then
    /// a release that carries every other pinned component forward. One
    /// command from "the site exported it" to "upload the origin".
    Roster {
        /// Where the site put it: a URL, or a local .rebornroster path.
        #[arg(long)]
        from: String,
        #[arg(long)]
        origin: PathBuf,
        /// The roster's label, for example 2026.09.06.
        #[arg(long)]
        version: String,
        /// Defaults to one above the highest roster sequence in the origin.
        #[arg(long)]
        sequence: Option<u64>,
        /// The publisher.key file. Without it the component is published and
        /// the release step is only described.
        #[arg(long)]
        key: Option<PathBuf>,
        /// The release label players see. Defaults to the one already served.
        #[arg(long)]
        release_version: Option<String>,
        /// Defaults to one above the release already served.
        #[arg(long)]
        release_sequence: Option<u64>,
        /// Write and sign. Without this everything is fetched and checked and
        /// nothing in the origin changes.
        #[arg(long)]
        apply: bool,
        /// A file holding the site's API token, for an export URL that needs
        /// a login. Not the token itself: a token on the command line is in
        /// every process list. The vault entry set by `publish token` is
        /// used when this is absent.
        #[arg(long)]
        token_file: Option<PathBuf>,
    },
    /// Keep the site's API token in the platform credential vault, so
    /// `publish roster` can fetch a login-only export without the token ever
    /// appearing on a command line or in a file the repository could see.
    Token {
        /// A file holding the token. Read once; delete it afterwards.
        #[arg(long)]
        from_file: Option<PathBuf>,
        /// Remove the stored token.
        #[arg(long)]
        forget: bool,
    },
}

/// The vault account the site's API token is kept under.
const SITE_TOKEN_ACCOUNT: &str = "nsr.site.export";

pub fn run(cmd: PublishCommand) -> reborn_core::Result<std::process::ExitCode> {
    match cmd {
        PublishCommand::Keygen { out } => keygen(&out),
        PublishCommand::Component {
            root,
            component,
            version,
            sequence,
            install_root,
            origin,
            apply,
        } => publish_component(&root, &component, &version, sequence, install_root.as_deref(), &origin, apply),
        PublishCommand::Release {
            origin,
            version,
            sequence,
            key,
            components,
            apply,
        } => release(&origin, &version, sequence, &key, &components, apply),
        PublishCommand::Roster {
            from,
            origin,
            version,
            sequence,
            key,
            release_version,
            release_sequence,
            apply,
            token_file,
        } => publish_roster(
            &from,
            &origin,
            &version,
            sequence,
            key.as_deref(),
            release_version.as_deref(),
            release_sequence,
            apply,
            token_file.as_deref(),
        ),
        PublishCommand::Token { from_file, forget } => site_token(from_file.as_deref(), forget),
    }
}

/// Store or forget the site's API token in the platform vault.
fn site_token(from_file: Option<&Path>, forget: bool) -> reborn_core::Result<std::process::ExitCode> {
    let platform = reborn_core::Platform::detect();
    if forget {
        platform.credentials.delete(SITE_TOKEN_ACCOUNT)?;
        println!("Forgot the site token ({SITE_TOKEN_ACCOUNT}).");
        return Ok(OK);
    }
    let Some(path) = from_file else {
        let present = platform.credentials.retrieve(SITE_TOKEN_ACCOUNT)?.is_some();
        println!(
            "Site token ({SITE_TOKEN_ACCOUNT}): {}. Pass --from-file <file> to set it, --forget to remove it.",
            if present { "stored" } else { "not stored" }
        );
        return Ok(OK);
    };
    let text = std::fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    let token = text.trim();
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        println!("{} does not hold one token on one line. Nothing was stored.", path.display());
        return Ok(PROBLEM);
    }
    platform.credentials.store(SITE_TOKEN_ACCOUNT, token)?;
    println!(
        "Stored the site token ({SITE_TOKEN_ACCOUNT}), {} characters. Delete {} now; the vault is its home.",
        token.len(),
        path.display()
    );
    Ok(OK)
}

// ---------------------------------------------------------------------------
// keygen
// ---------------------------------------------------------------------------

fn keygen(out: &Path) -> reborn_core::Result<std::process::ExitCode> {
    let publisher_path = out.join(PUBLISHER_KEY);
    let rotation_path = out.join(ROTATION_KEY);
    for p in [&publisher_path, &rotation_path] {
        if p.exists() {
            println!("{} already exists. This tool never overwrites a key: a key that", p.display());
            println!("is replaced by accident is a key that can no longer verify anything");
            println!("already published. Move it aside deliberately if a new one is wanted.");
            return Ok(PROBLEM);
        }
    }
    std::fs::create_dir_all(out).map_err(|e| io_err(out, e))?;

    let everyday = publisher::generate()?;
    let rotation = publisher::generate()?;
    write_key(&publisher_path, &everyday.secret)?;
    write_key(&rotation_path, &rotation.secret)?;

    println!("Wrote {}", publisher_path.display());
    println!("Wrote {}", rotation_path.display());
    println!();
    println!("Paste BOTH public keys into TRUSTED_KEYS in crates/reborn-core/src/signing.rs,");
    println!("everyday first. A launcher built with them will accept releases signed by");
    println!("either, which is what makes a rotation a launcher release and not a reinstall.");
    println!();
    println!("    // everyday   {}", signing::encode_public_key(&everyday.public));
    println!("    {},", rust_array(&everyday.public));
    println!("    // rotation   {}", signing::encode_public_key(&rotation.public));
    println!("    {},", rust_array(&rotation.public));
    println!();
    println!("Then:");
    println!("  {}  stays on the machine that publishes. Back it up. Never commit it.", PUBLISHER_KEY);
    println!("  {}   goes OFFLINE now: a USB stick in a drawer, or printed. It is only", ROTATION_KEY);
    println!("                 ever used if {} is lost or leaks, and a rotation key that", PUBLISHER_KEY);
    println!("                 sat beside the everyday key leaks in the same breach.");
    Ok(OK)
}

fn write_key(path: &Path, secret: &[u8; publisher::SECRET_KEY_LEN]) -> reborn_core::Result<()> {
    let text = format!("{}\n", publisher::encode_secret(secret));
    std::fs::write(path, text).map_err(|e| io_err(path, e))
}

fn rust_array(key: &[u8; signing::PUBLIC_KEY_LEN]) -> String {
    let items: Vec<String> = key.iter().map(|b| format!("0x{b:02x}")).collect();
    format!("[{}]", items.join(", "))
}

fn read_key(path: &Path) -> reborn_core::Result<Option<[u8; publisher::SECRET_KEY_LEN]>> {
    let text = std::fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    Ok(publisher::decode_secret(&text))
}

// ---------------------------------------------------------------------------
// component
// ---------------------------------------------------------------------------

fn publish_component(
    root: &Path,
    name: &str,
    version: &str,
    sequence: u64,
    install_root: Option<&str>,
    origin: &Path,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let Some(id) = ComponentId::parse(name) else {
        eprintln!("--component {name:?} is not one of the seven. Use one of:");
        for known in ComponentId::ALL {
            eprintln!("  {}", known.as_str());
        }
        return Ok(misuse());
    };
    let spec = match install_root {
        Some(given) => ComponentSpec::with_install_root(id, version, sequence, given),
        None => ComponentSpec::new(id, version, sequence),
    };
    let spec = match spec {
        Ok(s) => s,
        Err(e) => {
            println!("{e}");
            if install_root.is_none() {
                println!();
                println!("Pass --install-root <relative path> to state it.");
            }
            return Ok(PROBLEM);
        }
    };

    let manifest_path = manifest_path(origin, spec.id(), spec.version());
    println!("Tree          {}", root.display());
    println!("Component     {} version {}, sequence {}", spec.id().as_str(), spec.version(), spec.sequence());
    println!("Install root  {}", spec.install_root());
    println!("Origin        {}", origin.display());
    println!("Manifest      {}", manifest_path.display());
    println!();
    println!("Hashing. Progress goes to standard error. The tree must be still.");

    let log = ScanLog::new();
    let scan = match scan_component(root, &spec, &log)? {
        ScanOutcome::Cancelled { .. } => {
            println!("Cancelled. Nothing was written.");
            return Ok(PROBLEM);
        }
        ScanOutcome::Complete(scan) => scan,
    };
    println!(
        "Hashed {} file(s), {} bytes, in {:.1} s.",
        thousands(scan.progress.files_hashed),
        thousands(scan.progress.bytes_hashed),
        log.seconds()
    );
    let serialized = SerializedComponent::new(scan.component)?;

    // A manifest already at this name must be THIS manifest. Replacing it
    // would leave every client holding the old release pinning a hash no
    // served file has.
    if manifest_path.is_file() {
        let existing = std::fs::read(&manifest_path).map_err(|e| io_err(&manifest_path, e))?;
        if existing != serialized.bytes() {
            println!();
            println!("{} already exists with DIFFERENT contents.", manifest_path.display());
            println!("A published manifest is never rewritten: a client that has the release");
            println!("pinning it would report the new file as tampering. Publish a new version");
            println!("with a higher sequence instead. Nothing was written.");
            return Ok(PROBLEM);
        }
        println!("Manifest already published with identical bytes; it will not be rewritten.");
    }

    // The blob plan: which of these files the origin already holds.
    let mut new_files = 0u64;
    let mut new_bytes = 0u64;
    let mut shared = 0u64;
    let mut shared_bytes = 0u64;
    let mut to_copy: Vec<(&reborn_core::manifest::ManifestFile, PathBuf)> = Vec::new();
    for file in &serialized.component().files {
        let dst = origin.join(blob_path(&file.sha256));
        match std::fs::metadata(&dst) {
            Ok(m) if m.len() == file.bytes => {
                shared += 1;
                shared_bytes += file.bytes;
            }
            Ok(m) => {
                println!();
                println!("{} exists with {} bytes; the manifest says {}.", dst.display(), m.len(), file.bytes);
                println!("A content-addressed blob cannot have two lengths. Something wrote a wrong");
                println!("or truncated file at that name. Delete it and run again. Nothing was written.");
                return Ok(PROBLEM);
            }
            Err(_) => {
                new_files += 1;
                new_bytes += file.bytes;
                to_copy.push((file, dst));
            }
        }
    }
    println!();
    println!(
        "Blobs         {} new ({} bytes), {} already in the origin ({} bytes)",
        thousands(new_files),
        thousands(new_bytes),
        thousands(shared),
        thousands(shared_bytes)
    );
    if shared > 0 {
        println!("              Shared blobs are uploaded once and fetched once, whichever");
        println!("              component or version references them.");
    }

    if !apply {
        println!();
        println!("Nothing was written. Re-run with --apply to write the manifest and the");
        println!("{} new blob(s).", thousands(new_files));
        return Ok(OK);
    }

    // Blobs first, manifest last. A manifest naming blobs that are not there
    // yet is a release that fails on every client; blobs with no manifest are
    // just bytes nobody asks for.
    let started = std::time::Instant::now();
    let mut copied_bytes = 0u64;
    for (i, (file, dst)) in to_copy.iter().enumerate() {
        let src = root.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        copy_blob(&src, dst, &file.sha256, file.bytes)?;
        copied_bytes += file.bytes;
        if (i + 1) % 500 == 0 || i + 1 == to_copy.len() {
            eprintln!(
                "  copied {} of {} blob(s), {} bytes, {:.0} s",
                thousands((i + 1) as u64),
                thousands(to_copy.len() as u64),
                thousands(copied_bytes),
                started.elapsed().as_secs_f64()
            );
        }
    }
    if !manifest_path.is_file() {
        serialized.write_to(&manifest_path)?;
    }

    println!();
    println!("Wrote {} blob(s) and {}", thousands(new_files), manifest_path.display());
    println!("Each new blob was hashed again after the copy; the copy is what is served.");
    println!("Pin it in a release with:");
    println!("  rebornctl publish release --origin {} --component {}={} ...", origin.display(), spec.id().as_str(), spec.version());
    Ok(OK)
}

fn manifest_path(origin: &Path, id: ComponentId, version: &str) -> PathBuf {
    origin.join(COMPONENTS_DIR).join(format!("{}-{}.json", id.as_str(), version))
}

/// The address a release pins a component manifest at, relative to the origin.
fn manifest_url(id: ComponentId, version: &str) -> String {
    format!("{COMPONENTS_DIR}/{}-{}.json", id.as_str(), version)
}

/// Copy one file into the blob store and prove the copy is the file.
///
/// Written to a temporary name and renamed, so a file at the final name is
/// always a verified one. Hashed after the copy because the copy is what is
/// served, and the scan's hash only proves what the source was.
fn copy_blob(src: &Path, dst: &Path, want: &Sha256Hex, want_bytes: u64) -> reborn_core::Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let tmp = dst.with_extension("part");
    let copied = std::fs::copy(src, &tmp).map_err(|e| io_err(src, e))?;
    let got = hash_file(&tmp)?;
    if copied != want_bytes || &got != want {
        let _ = std::fs::remove_file(&tmp);
        return Err(reborn_core::Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{} changed between the scan and the copy: scanned {want} at {want_bytes} bytes, \
                 copied {got} at {copied} bytes. The tree is not still. Nothing was published \
                 for it; run again on a quiet tree",
                src.display()
            ),
        )));
    }
    std::fs::rename(&tmp, dst).map_err(|e| io_err(dst, e))
}

// ---------------------------------------------------------------------------
// release
// ---------------------------------------------------------------------------

fn release(
    origin: &Path,
    version: &str,
    sequence: u64,
    key: &Path,
    components: &[String],
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    if components.is_empty() {
        eprintln!("--component NAME=VERSION is required at least once. A release pinning");
        eprintln!("nothing is refused by the builder and useless to a client.");
        return Ok(misuse());
    }

    // The key first, because everything else is wasted without it.
    let Some(secret) = read_key(key)? else {
        println!("{} does not hold a key: expected 64 hex characters.", key.display());
        return Ok(PROBLEM);
    };
    let public = publisher::public_of(&secret);
    let public_hex = signing::encode_public_key(&public);
    let shipped = signing::TRUSTED_KEYS.contains(&public);

    // Replay protection at the publisher, mirroring the client's rule: a
    // release that is not strictly newer than the one already served would be
    // refused by every launcher that has seen the current one.
    let release_path = origin.join(RELEASE_FILE);
    if release_path.is_file() {
        let bytes = std::fs::read(&release_path).map_err(|e| io_err(&release_path, e))?;
        match Release::parse(&bytes) {
            Ok(current) if sequence <= current.sequence => {
                println!(
                    "The origin already serves release {} at sequence {}. A new release must",
                    current.version, current.sequence
                );
                println!("carry a strictly higher sequence, or every launcher that has seen the");
                println!("current one refuses it as a replay. Nothing was written.");
                return Ok(PROBLEM);
            }
            Ok(current) => println!("Replacing     release {} (sequence {})", current.version, current.sequence),
            Err(e) => {
                println!("{} exists but does not parse: {e}", release_path.display());
                println!("Refusing to write over something this tool cannot read. Move it aside.");
                return Ok(PROBLEM);
            }
        }
    }

    println!("Origin        {}", origin.display());
    println!("Release       {version}, sequence {sequence}");
    println!("Signing key   {} (public {})", key.display(), public_hex);
    println!();

    let mut builder = UnsignedReleaseBuilder::new(version, sequence)?;
    let mut total_files = 0u64;
    let mut total_bytes = 0u64;
    for spec in components {
        let Some((name, ver)) = spec.split_once('=') else {
            eprintln!("--component {spec:?}: expected NAME=VERSION.");
            return Ok(misuse());
        };
        let Some(id) = ComponentId::parse(name) else {
            eprintln!("--component {spec:?}: {name:?} is not one of the seven.");
            return Ok(misuse());
        };
        let path = manifest_path(origin, id, ver);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                println!("{}: {e}", path.display());
                println!("Publish the component first: rebornctl publish component --component {name} --version {ver} ...");
                return Ok(PROBLEM);
            }
        };
        let parsed = Component::parse(&bytes).map_err(|e| {
            reborn_core::Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{} is not a component manifest: {e}", path.display()),
            ))
        })?;
        if parsed.component != name || parsed.version != ver {
            println!(
                "{} says it is {} {}, not {name} {ver}. The file name and its contents disagree;",
                path.display(),
                parsed.component,
                parsed.version
            );
            println!("nothing was pinned.");
            return Ok(PROBLEM);
        }

        // The pin must be the hash of the FILE. SerializedComponent hashes its
        // own serialization, so prove that serialization is byte for byte the
        // file on disk before trusting its hash.
        let serialized = SerializedComponent::new(parsed)?;
        let on_disk = Sha256Hex::from_bytes(&bytes);
        if serialized.sha256() != &on_disk {
            println!("{} does not round-trip: the file hashes to {on_disk} but its", path.display());
            println!("serialization hashes to {}. This tool did not write it, or something", serialized.sha256());
            println!("reformatted it. Pinning either hash would break every client; nothing was pinned.");
            return Ok(PROBLEM);
        }

        // Every blob the manifest names, present at its published length.
        let mut missing = Vec::new();
        for file in &serialized.component().files {
            let blob = origin.join(blob_path(&file.sha256));
            match std::fs::metadata(&blob) {
                Ok(m) if m.len() == file.bytes => {}
                _ => missing.push(file.path.clone()),
            }
        }
        if !missing.is_empty() {
            println!("{name} {ver}: {} of {} blob(s) are not in the origin at their published", missing.len(), serialized.component().files.len());
            println!("length, starting with {:?}. A release must never name content the origin", missing[0]);
            println!("does not hold. Re-run publish component for it. Nothing was pinned.");
            return Ok(PROBLEM);
        }

        let files = serialized.component().files.len() as u64;
        let bytes_total = serialized.component().total_bytes();
        total_files += files;
        total_bytes += bytes_total;
        println!(
            "Pin           {name} {ver}: {} file(s), {} bytes, manifest sha256 {}",
            thousands(files),
            thousands(bytes_total),
            serialized.sha256()
        );
        builder = builder.pin(&manifest_url(id, ver), &serialized)?;
    }
    let unsigned = builder.build()?;
    let signature = publisher::sign(&secret, unsigned.bytes_to_sign());
    let sig_text = format!("{}\n", signing::encode_signature(&signature));

    println!();
    println!("Total         {} file(s), {} bytes across {} component(s)", thousands(total_files), thousands(total_bytes), components.len());
    println!("Would write   {}", release_path.display());
    println!("Would write   {}", origin.join(SIGNATURE_FILE).display());
    if !shipped {
        println!();
        println!("WARNING: this key's public half is not in TRUSTED_KEYS in signing.rs, so no");
        println!("shipped launcher will accept this release. A launcher built after pasting");
        println!("  {public_hex}");
        println!("into TRUSTED_KEYS will. rebornctl update content --trust <hex> can test it now.");
    }

    if !apply {
        println!();
        println!("Nothing was written. Re-run with --apply to write and sign.");
        return Ok(OK);
    }

    // Signature first, then the release: a release.json with a stale or
    // absent .sig beside it is an outage for every client, and a .sig with no
    // matching release is a file nobody reads.
    let sig_path = origin.join(SIGNATURE_FILE);
    std::fs::write(&sig_path, &sig_text).map_err(|e| io_err(&sig_path, e))?;
    unsigned.write_to(&release_path)?;

    // Read both back and verify, the way a launcher will.
    let served = std::fs::read(&release_path).map_err(|e| io_err(&release_path, e))?;
    let served_sig = std::fs::read_to_string(&sig_path).map_err(|e| io_err(&sig_path, e))?;
    let decoded = signing::decode_signature(&served_sig).ok_or_else(|| {
        reborn_core::Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the signature file this tool just wrote does not decode",
        ))
    })?;
    let verifier = Ed25519Verifier::with_keys(&[public])?;
    let trusted = TrustedRelease::open(&verifier, &served, &decoded)?;

    println!();
    println!("Wrote {}", release_path.display());
    println!("Wrote {}", sig_path.display());
    println!(
        "Read back and verified: release {} sequence {} opens with public key {}",
        trusted.release().version,
        trusted.release().sequence,
        public_hex
    );
    println!("That is the file a launcher fetches. Upload the origin directory as it is.");
    Ok(OK)
}

// ---------------------------------------------------------------------------
// roster
// ---------------------------------------------------------------------------

/// Fetch a roster and push it as the `roster` component, then re-sign a
/// release that pins it beside everything already served.
#[allow(clippy::too_many_arguments)]
fn publish_roster(
    from: &str,
    origin: &Path,
    version: &str,
    sequence: Option<u64>,
    key: Option<&Path>,
    release_version: Option<&str>,
    release_sequence: Option<u64>,
    apply: bool,
    token_file: Option<&Path>,
) -> reborn_core::Result<std::process::ExitCode> {
    use reborn_core::net::Http as _;
    use reborn_core::roster::ROSTER_FILE;

    // A folder holding exactly the one file, because the component scan
    // hashes everything under its root.
    let stage = std::env::temp_dir().join("reborn-publish").join("roster").join(version);
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|e| io_err(&stage, e))?;
    }
    std::fs::create_dir_all(&stage).map_err(|e| io_err(&stage, e))?;
    let staged = stage.join(ROSTER_FILE);

    if from.starts_with("http://") || from.starts_with("https://") {
        // The token: the vault first, then a file. Its value is never
        // printed; only whether one was sent.
        let token: Option<String> = match token_file {
            Some(p) => {
                let t = std::fs::read_to_string(p).map_err(|e| io_err(p, e))?;
                Some(t.trim().to_string()).filter(|t| !t.is_empty())
            }
            None => reborn_core::Platform::detect()
                .credentials
                .retrieve(SITE_TOKEN_ACCOUNT)
                .unwrap_or(None),
        };
        println!(
            "Fetching      {from}{}",
            if token.is_some() { "  (with the site token)" } else { "" }
        );
        let http = reborn_core::net::UreqHttp::new(concat!("rebornctl/", env!("CARGO_PKG_VERSION")));
        let bytes = match http.get_to_file_as(from, &staged, token.as_deref()) {
            Ok(b) => b,
            Err(e) if e.to_string().contains("answered 401") => {
                println!();
                println!("The site answered 401: that export needs a login. Store the site's API token");
                println!("with `rebornctl publish token --from-file <file>` or pass --token-file. Nothing");
                println!("was published.");
                return Ok(PROBLEM);
            }
            Err(e) => return Err(e),
        };
        println!("Fetched       {} bytes", thousands(bytes));
    } else {
        let src = Path::new(from);
        if !src.is_file() {
            println!("{from} is neither a URL nor a file. Nothing was published.");
            return Ok(PROBLEM);
        }
        std::fs::copy(src, &staged).map_err(|e| io_err(src, e))?;
        println!("Roster        {}", src.display());
    }

    // The Builder is the authority on the format; this only refuses what
    // is plainly not a roster, so a 404 page saved as a file cannot be
    // published as one. A .rebornroster is a zip and its entry names sit in
    // the clear inside it.
    let head = std::fs::read(&staged).map_err(|e| io_err(&staged, e))?;
    let looks_like_zip = head.starts_with(b"PK\x03\x04");
    let names_present = [&b"manifest.json"[..], &b"players.json"[..], &b"teams.json"[..]]
        .iter()
        .all(|n| head.windows(n.len()).any(|w| w == *n));
    if !looks_like_zip || !names_present {
        println!();
        println!("That is not a .rebornroster: expected a zip holding manifest.json, players.json");
        println!("and teams.json. Got {} bytes starting {:?}. Nothing was published.", head.len(),
                 String::from_utf8_lossy(&head[..head.len().min(16)]));
        return Ok(PROBLEM);
    }

    let sequence = match sequence {
        Some(s) => s,
        None => {
            let next = next_roster_sequence(origin)?;
            println!("Sequence      {next} (one above the highest roster already in the origin)");
            next
        }
    };

    let published = publish_component(&stage, "roster", version, sequence, None, origin, apply)?;
    if published != OK {
        return Ok(published);
    }

    // The release. Every component already pinned is carried forward at
    // the version it is served at; the roster pin is replaced or added.
    let release_path = origin.join(RELEASE_FILE);
    let existing = if release_path.is_file() {
        let bytes = std::fs::read(&release_path).map_err(|e| io_err(&release_path, e))?;
        Some(Release::parse(&bytes).map_err(|e| {
            reborn_core::Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{} does not parse: {e}", release_path.display()),
            ))
        })?)
    } else {
        None
    };
    let mut pins: Vec<String> = Vec::new();
    if let Some(current) = &existing {
        for (name, pinned) in &current.components {
            if name == "roster" {
                continue;
            }
            let manifest = origin.join(pinned.url.trim_start_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR));
            let bytes = std::fs::read(&manifest).map_err(|e| io_err(&manifest, e))?;
            let component = Component::parse(&bytes).map_err(|e| {
                reborn_core::Error::PlainIo(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} does not parse: {e}", manifest.display()),
                ))
            })?;
            pins.push(format!("{name}={}", component.version));
        }
    }
    pins.push(format!("roster={version}"));
    pins.sort();

    let (rv, rs) = match (&existing, release_version, release_sequence) {
        (Some(c), v, s) => (
            v.map(str::to_string).unwrap_or_else(|| c.version.clone()),
            s.unwrap_or(c.sequence + 1),
        ),
        (None, Some(v), Some(s)) => (v.to_string(), s),
        (None, _, _) => {
            println!();
            println!("The origin serves no release yet, so there is nothing to carry forward.");
            println!("Pass --release-version and --release-sequence, or run publish release");
            println!(
                "with every component. The roster component itself {}.",
                if apply { "was published" } else { "would be published with --apply" }
            );
            return Ok(OK);
        }
    };

    let Some(key) = key else {
        println!();
        println!("No --key, so the release was not signed. To pin it, run:");
        println!(
            "  rebornctl publish release --origin {} --version {rv} --sequence {rs} --key <publisher.key> {} --apply",
            origin.display(),
            pins.iter().map(|p| format!("--component {p}")).collect::<Vec<_>>().join(" ")
        );
        return Ok(OK);
    };

    println!();
    println!("Release       {rv}, sequence {rs}, pinning {}", pins.join(", "));
    release(origin, &rv, rs, key, &pins, apply)
}

/// One above the highest sequence of any roster manifest in the origin.
fn next_roster_sequence(origin: &Path) -> reborn_core::Result<u64> {
    let dir = origin.join(COMPONENTS_DIR);
    let mut highest = 0u64;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("roster-") || !name.ends_with(".json") {
                continue;
            }
            let bytes = std::fs::read(entry.path()).map_err(|e| io_err(&entry.path(), e))?;
            if let Ok(c) = Component::parse(&bytes) {
                highest = highest.max(c.sequence);
            }
        }
    }
    Ok(highest + 1)
}

fn io_err(path: &Path, source: std::io::Error) -> reborn_core::Error {
    reborn_core::Error::Io { path: path.to_path_buf(), source }
}
