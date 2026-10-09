//! The "League files" page: publish the league build (`league-game`) or the
//! league seed (`league-content`), upload their blobs to the PRIVATE bucket
//! first, then the manifest and release to the public bucket. One button per
//! job, like every page.
//!
//! # Members-only blobs never go public
//!
//! `reborn_core::publish::league` stores these components' blobs under
//! `<origin>/private/blobs/by-hash`, which `plan_origin` never reads. Only
//! [`plan_private`] reads it, and only [`upload_private_then_public`] calls
//! that, against the bucket [`private_bucket_name`] approved: a canonical
//! S3 name (the same rule `rebornctl publish upload --private` uses), never
//! the production bucket and never the public bucket this machine's settings
//! name. The private upload has no `public_base`, so nothing is ever read
//! back through the public CDN.
//!
//! # Order
//!
//! Private blobs first, then the public plan (the manifest and the two
//! release files). If the private upload is refused or fails, the public
//! upload never starts: a public release pinning blobs members cannot fetch
//! would break every league install at once.
//!
//! # The check
//!
//! The check is the same command with `dry_run: true`, journalled as a
//! `plan` step (`league_game:plan`), so the job card shows "Checked ...
//! Nothing was written." It reads no vault and touches no network.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use reborn_core::inventory::{RunToCompletion, ScanObserver};
use reborn_core::net::{Http, UreqHttp};
use reborn_core::publish::league::{publish_league_content, publish_league_game, LeagueContentRequest, LeagueGameRequest};
use reborn_core::publish::roster::Push;
use reborn_core::publish::upload::{self, check_private_bucket, plan_origin, plan_private, Bucket, Transport, UploadRequest, UreqTransport};
use reborn_core::publish::{refuse, Event, Outcome, Sink};

use crate::content::{bucket_from, key_path, origin_dir, settings_and_platform};
use crate::ops;
use crate::settings::Settings;

fn trimmed(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// The private bucket members-only blobs go to, from settings. Refused when
/// absent, not a canonical bucket name, the production bucket, or the same
/// bucket this machine publishes public files to. Checked on the dry run
/// too, so a bad setting shows up on the check, not halfway through a
/// publish.
pub(crate) fn private_bucket_name(settings: &Settings) -> Result<String, String> {
    let Some(name) = settings.league_bucket_name.as_deref().filter(|b| !b.is_empty()) else {
        return Err("No private league bucket is configured. Set league_bucket_name (for example reborn-league) in \
                    Settings."
            .to_string());
    };
    let public: Vec<&str> = settings.bucket_name.as_deref().map(str::trim).filter(|b| !b.is_empty()).into_iter().collect();
    check_private_bucket(name, &public)?;
    Ok(name.to_string())
}

/// Private blobs first, then the public plan (manifest + release files).
/// A refused or failed private upload returns before anything public is
/// planned, listed or sent.
pub(crate) fn upload_private_then_public(
    transport: &dyn Transport,
    http: &dyn Http,
    public: &Bucket,
    private: &Bucket,
    origin: &Path,
    public_base: &str,
    sink: Sink<'_>,
) -> reborn_core::Result<Outcome<Value>> {
    if private.bucket == public.bucket {
        return Ok(refuse(sink, &[format!(
            "The private league bucket and the public bucket are both {}. Members-only blobs never go to the \
             public bucket. Nothing was uploaded.",
            public.bucket
        )]));
    }

    let (already_private, how) = upload::already_on_bucket(transport, private)?;
    sink(Event::out(how));
    let private_plan = plan_private(origin, &*already_private)?;
    let req = UploadRequest {
        bucket: private,
        plan: &private_plan,
        multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD,
        part_size: upload::DEFAULT_PART_SIZE,
        attempts: upload::DEFAULT_ATTEMPTS,
        // Never read back: the public CDN does not serve these.
        public_base: None,
        apply: true,
    };
    let private_done = match upload::upload(transport, http, &req, sink)? {
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Done(u) => u,
        Outcome::Described => Default::default(),
    };

    let (already_public, how) = upload::already_on_bucket(transport, public)?;
    sink(Event::out(how));
    let public_plan = plan_origin(origin, &*already_public)?;
    let req = UploadRequest {
        bucket: public,
        plan: &public_plan,
        multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD,
        part_size: upload::DEFAULT_PART_SIZE,
        attempts: upload::DEFAULT_ATTEMPTS,
        public_base: Some(public_base),
        apply: true,
    };
    let public_done = match upload::upload(transport, http, &req, sink)? {
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Done(u) => u,
        Outcome::Described => Default::default(),
    };
    Ok(Outcome::Done(json!({
        "private_objects": private_done.objects, "private_bytes": private_done.bytes,
        "public_objects": public_done.objects, "public_bytes": public_done.bytes,
        "verified": public_done.verified.first().cloned(),
    })))
}

/// Everything a run resolves before its body starts. `key` and `buckets`
/// are `None` on a dry run: the check reads no key and no vault.
struct Resolved {
    origin: PathBuf,
    public_base: String,
    key: Option<PathBuf>,
    buckets: Option<(Bucket, Bucket)>,
}

fn resolve(dry_run: bool) -> Result<Resolved, String> {
    let (platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let public_base = settings.public_base.clone();
    let league = private_bucket_name(&settings)?;
    if dry_run {
        return Ok(Resolved { origin, public_base, key: None, buckets: None });
    }
    let key = key_path(&settings)?;
    let public = bucket_from(&platform, &settings)?;
    let private = Bucket { bucket: league, ..public.clone() };
    Ok(Resolved { origin, public_base, key: Some(key), buckets: Some((public, private)) })
}

fn finish(push: Outcome<Push>, what: &str, resolved: &Resolved, sink: Sink<'_>) -> reborn_core::Result<Outcome<Value>> {
    let p = match push {
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Described => {
            let summary = format!("Checked {what}. Nothing was written.");
            sink(Event::out(summary.clone()));
            return Ok(Outcome::Done(json!({"summary": summary})));
        }
        Outcome::Done(p) => p,
    };
    let Some(signed) = p.release.as_ref() else {
        return Ok(refuse(sink, &[format!(
            "{what} was written to the origin, but no release was signed, so nothing was uploaded. Sign a release, \
             then publish again."
        )]));
    };
    let Some((public, private)) = resolved.buckets.as_ref() else {
        return Ok(refuse(sink, &["No bucket was resolved for this real publish. Nothing was uploaded.".to_string()]));
    };
    let transport = UreqTransport::new();
    let http = UreqHttp::new(concat!("reborn-publisher/", env!("CARGO_PKG_VERSION")));
    match upload_private_then_public(&transport, &http, public, private, &resolved.origin, &resolved.public_base, sink)? {
        Outcome::Done(v) => Ok(Outcome::Done(json!({
            "summary": format!("Published {what}: release sequence {}. Members can download it now.", signed.sequence),
            "sequence": signed.sequence,
            "verified": v.get("verified").cloned().unwrap_or(Value::Null),
            "upload": v,
        }))),
        other => Ok(other),
    }
}

/// The op string: a dry run is this page's check (a `plan` step), anything
/// else its run.
fn op_name(page: &'static str, dry_run: bool) -> &'static str {
    match (page, dry_run) {
        ("league_game", true) => "league_game:plan",
        ("league_game", false) => "league_game:run",
        ("league_content", true) => "league_content:plan",
        _ => "league_content:run",
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn league_game_run(iso: String, textures: String, version: String, notes: String, dry_run: bool) -> Result<Value, String> {
    let iso = PathBuf::from(trimmed(&iso).ok_or("Choose the league disc first.")?);
    let textures = PathBuf::from(trimmed(&textures).ok_or("Choose the league texture folder first.")?);
    let version = trimmed(&version).ok_or("Choose a version first.")?;
    let notes = trimmed(&notes);
    let resolved = resolve(dry_run)?;
    let inputs = json!({
        "kind": "league-game-run", "iso": iso.display().to_string(), "textures": textures.display().to_string(),
        "version": version, "dry_run": dry_run,
    });
    ops::run_operation(op_name("league_game", dry_run), dry_run, inputs, move |sink: Sink<'_>| {
        let req = LeagueGameRequest {
            iso: &iso,
            textures: &textures,
            version: &version,
            stage: None,
            origin: &resolved.origin,
            sequence: None,
            key: resolved.key.as_deref(),
            release_version: None,
            release_sequence: None,
            notes: notes.as_deref(),
            apply: !dry_run,
        };
        let push = publish_league_game(&req, &RunToCompletion as &dyn ScanObserver, sink)?;
        finish(push, &format!("league-game {version}"), &resolved, sink)
    })
    .map(|()| json!({"accepted": true}))
}

#[tauri::command(rename_all = "snake_case")]
pub fn league_content_run(seed: String, seed_json: Option<String>, version: String, notes: String, dry_run: bool) -> Result<Value, String> {
    let seed = PathBuf::from(trimmed(&seed).ok_or("Choose the league seed (.s01) first.")?);
    let seed_json = seed_json.as_deref().and_then(trimmed).map(PathBuf::from);
    let version = trimmed(&version).ok_or("Choose a version first.")?;
    let notes = trimmed(&notes);
    let resolved = resolve(dry_run)?;
    let inputs = json!({
        "kind": "league-content-run", "seed": seed.display().to_string(),
        "seed_json": seed_json.as_ref().map(|p| p.display().to_string()), "version": version, "dry_run": dry_run,
    });
    ops::run_operation(op_name("league_content", dry_run), dry_run, inputs, move |sink: Sink<'_>| {
        let req = LeagueContentRequest {
            seed: &seed,
            seed_json: seed_json.as_deref(),
            version: &version,
            stage: None,
            origin: &resolved.origin,
            sequence: None,
            key: resolved.key.as_deref(),
            release_version: None,
            release_sequence: None,
            notes: notes.as_deref(),
            apply: !dry_run,
        };
        let push = publish_league_content(&req, &RunToCompletion as &dyn ScanObserver, sink)?;
        finish(push, &format!("league-content {version}"), &resolved, sink)
    })
    .map(|()| json!({"accepted": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reborn_core::manifest::Sha256Hex;
    use reborn_core::net::HttpResponse;
    use reborn_core::origin::{blob_path, private_blob_path, RELEASE_FILE, SIGNATURE_FILE};
    use reborn_core::publish::upload::{Body, Reply, SignedRequest};
    use std::sync::Mutex;

    fn settings(league: Option<&str>, public: Option<&str>) -> Settings {
        Settings {
            league_bucket_name: league.map(str::to_string),
            bucket_name: public.map(str::to_string),
            ..Settings::default()
        }
    }

    #[test]
    fn the_default_league_bucket_is_accepted() {
        assert_eq!(private_bucket_name(&Settings::default()).unwrap(), "reborn-league");
        assert_eq!(private_bucket_name(&settings(Some("reborn-league"), Some("reborn-content"))).unwrap(), "reborn-league");
    }

    #[test]
    fn the_league_bucket_is_refused_when_missing_public_or_not_canonical() {
        assert!(private_bucket_name(&settings(None, None)).is_err());
        assert!(private_bucket_name(&settings(Some(""), None)).is_err());
        // The production bucket, however this machine's public bucket is named.
        assert!(private_bucket_name(&settings(Some("reborn-content"), Some("some-test-bucket"))).is_err());
        // The very bucket this machine publishes public files to.
        let same = private_bucket_name(&settings(Some("my-public"), Some("my-public"))).unwrap_err();
        assert!(same.contains("PUBLIC"), "{same}");
        for bad in [" reborn-league", "reborn-league ", "Reborn-League", "reborn.league", "a/b", "../x", "ab", "-reborn"] {
            assert!(private_bucket_name(&settings(Some(bad), None)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn op_names_make_the_dry_run_a_plan_step() {
        assert_eq!(op_name("league_game", true), "league_game:plan");
        assert_eq!(op_name("league_game", false), "league_game:run");
        assert_eq!(op_name("league_content", true), "league_content:plan");
        assert_eq!(op_name("league_content", false), "league_content:run");
    }

    /// Records every request; refuses PUTs to `fail_bucket` with a 403.
    /// Listings answer "empty bucket".
    #[derive(Debug, Default)]
    struct Recorder {
        calls: Mutex<Vec<(String, String)>>,
        fail_bucket: Option<String>,
    }
    impl Transport for Recorder {
        fn send(&self, req: &SignedRequest, _body: Body<'_>) -> reborn_core::Result<Reply> {
            self.calls.lock().unwrap().push((req.method.to_string(), req.url.clone()));
            if req.method == "GET" {
                let xml = "<?xml version=\"1.0\"?><ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>";
                return Ok(Reply { status: 200, headers: vec![], body: xml.as_bytes().to_vec() });
            }
            if let Some(b) = &self.fail_bucket {
                if req.url.contains(&format!("/{b}/")) {
                    return Ok(Reply { status: 403, headers: vec![], body: b"AccessDenied".to_vec() });
                }
            }
            Ok(Reply { status: 200, headers: vec![], body: vec![] })
        }
    }

    /// The public read-back: serves the origin's own release.json.
    struct Edge(Vec<u8>);
    impl Http for Edge {
        fn get_text(&self, _url: &str) -> reborn_core::Result<HttpResponse> {
            Ok(HttpResponse { status: 200, body: String::from_utf8(self.0.clone()).unwrap() })
        }
        fn get_to_file(&self, url: &str, _to: &Path) -> reborn_core::Result<u64> {
            panic!("no download expected: {url}")
        }
        fn post_json(&self, url: &str, _body: &[u8], _bearer: Option<&str>) -> reborn_core::Result<HttpResponse> {
            panic!("no post expected: {url}")
        }
    }

    fn bucket(name: &str) -> Bucket {
        Bucket {
            endpoint: "https://acct.r2.cloudflarestorage.com".into(),
            bucket: name.into(),
            region: "auto".into(),
            access_key_id: "AKIA".into(),
            secret_access_key: "secret".into(),
        }
    }

    /// An origin holding one members-only blob, one public blob, one
    /// manifest and a signed release.
    fn origin() -> (tempfile::TempDir, Sha256Hex, Sha256Hex) {
        let dir = tempfile::tempdir().unwrap();
        let o = dir.path();
        let write = |rel: &str, bytes: &[u8]| {
            let p = o.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        };
        let private = Sha256Hex::from_bytes(b"league disc");
        let public = Sha256Hex::from_bytes(b"roster");
        write(&private_blob_path(&private), b"league disc");
        write(&blob_path(&public), b"roster");
        write("components/league-game-1.0.0.json", b"{}");
        write(RELEASE_FILE, b"{\"release\":1}");
        write(SIGNATURE_FILE, b"sig");
        (dir, private, public)
    }

    fn puts(calls: &[(String, String)]) -> Vec<String> {
        calls.iter().filter(|(m, _)| m == "PUT").map(|(_, u)| u.clone()).collect()
    }

    #[test]
    fn private_blobs_go_up_first_and_only_to_the_private_bucket() {
        let (dir, private, public) = origin();
        let t = Recorder::default();
        let edge = Edge(std::fs::read(dir.path().join(RELEASE_FILE)).unwrap());
        let mut sink = |_e: Event| {};
        let out = upload_private_then_public(&t, &edge, &bucket("reborn-content"), &bucket("reborn-league"), dir.path(), "https://files.example.test/", &mut sink).unwrap();
        assert!(matches!(out, Outcome::Done(_)), "{out:?}");

        let calls = t.calls.lock().unwrap().clone();
        let puts = puts(&calls);
        let first_public = puts.iter().position(|u| u.contains("/reborn-content/")).expect("a public put");
        let private_puts: Vec<&String> = puts.iter().filter(|u| u.contains("/reborn-league/")).collect();
        assert_eq!(private_puts.len(), 1, "{puts:?}");
        assert!(private_puts[0].contains(private.as_str()), "{puts:?}");
        assert!(puts[..first_public].iter().all(|u| u.contains("/reborn-league/")), "private first: {puts:?}");
        assert!(puts[first_public..].iter().all(|u| u.contains("/reborn-content/")), "then public only: {puts:?}");
        // The members-only blob never reaches the public bucket; the public plan
        // carries the public blob, the manifest and both release files.
        assert!(!puts.iter().any(|u| u.contains("/reborn-content/") && u.contains(private.as_str())), "{puts:?}");
        assert!(puts.iter().any(|u| u.contains("/reborn-content/") && u.contains(public.as_str())), "{puts:?}");
        assert!(puts.iter().any(|u| u.ends_with("/reborn-content/release.json")), "{puts:?}");
        assert!(puts.iter().any(|u| u.ends_with("/reborn-content/release.json.sig")), "{puts:?}");
        // The first listing is the private bucket's.
        assert!(calls[0].0 == "GET" && calls[0].1.contains("/reborn-league/"), "{calls:?}");
    }

    #[test]
    fn a_failed_private_upload_sends_nothing_public() {
        let (dir, _private, _public) = origin();
        let t = Recorder { fail_bucket: Some("reborn-league".into()), ..Default::default() };
        let edge = Edge(vec![]);
        let mut sink = |_e: Event| {};
        let out = upload_private_then_public(&t, &edge, &bucket("reborn-content"), &bucket("reborn-league"), dir.path(), "https://files.example.test/", &mut sink).unwrap();
        assert!(matches!(out, Outcome::Refused(_)), "{out:?}");
        let calls = t.calls.lock().unwrap().clone();
        assert!(!calls.iter().any(|(_, u)| u.contains("/reborn-content/")), "nothing public, not even a listing: {calls:?}");
    }

    #[test]
    fn the_same_bucket_for_both_is_refused_before_any_request() {
        let (dir, _p, _q) = origin();
        let t = Recorder::default();
        let edge = Edge(vec![]);
        let mut sink = |_e: Event| {};
        let out = upload_private_then_public(&t, &edge, &bucket("reborn-content"), &bucket("reborn-content"), dir.path(), "https://files.example.test/", &mut sink).unwrap();
        assert!(matches!(out, Outcome::Refused(_)), "{out:?}");
        assert!(t.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn a_dry_run_check_says_nothing_was_written() {
        let resolved = Resolved { origin: PathBuf::from("x"), public_base: String::new(), key: None, buckets: None };
        let mut lines = Vec::new();
        let mut sink = |e: Event| lines.push(format!("{e:?}"));
        let out = finish(Outcome::Described, "league-game 1.0.0", &resolved, &mut sink).unwrap();
        match out {
            Outcome::Done(v) => assert_eq!(v["summary"], "Checked league-game 1.0.0. Nothing was written."),
            other => panic!("{other:?}"),
        }
    }
}
