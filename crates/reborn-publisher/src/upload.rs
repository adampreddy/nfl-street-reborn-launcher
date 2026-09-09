//! The Status page's Upload command: push whatever this machine has
//! published locally that the bucket does not yet serve.
//!
//! Every other page's own `_run` command bundles a fresh publish (a fetch,
//! a build, a hash) together with an upload of exactly what it just
//! touched. The Status page has no publish of its own to bundle one into;
//! it only shows a `Pending` list next to an Upload button. `status.rs`'s
//! own module doc named this gap plainly: no command reachable from that
//! page could re-upload, because `main.rs`'s command list carried no
//! page-agnostic "upload whatever the origin holds" command. This file is
//! that command, as a plan/run pair over
//! `reborn_core::publish::upload::{plan_origin, upload}`, run through
//! [`crate::ops::run_operation`] exactly the way every other page's pair
//! is, so an upload started from Status is journalled like any other
//! operation.
//!
//! # Credentials
//!
//! Read from the vault with `reborn_core::publish::vault::{r2_access_key,
//! r2_secret_key}`, the same two accessors `history.rs`'s rollback upload
//! already uses. Neither command below takes a credential as an argument:
//! a `#[tauri::command]`'s arguments are exactly what a browser devtools
//! console sitting in front of this window could set to anything, and a
//! secret has no business being something a page can pass in.
//!
//! # What "the bucket already has this" means here
//!
//! [`upload::plan_origin`] decides what is pending by asking an `already`
//! callback for the length the bucket already serves each key at. This
//! file answers that with a signed HEAD straight to the bucket, the same
//! shape `history.rs`'s own `head_len` uses for its scoped rollback
//! upload, rather than reading the public CDN: an edge cache can lag
//! behind the bucket itself, and the plan should describe what a full
//! upload run would actually still need to send, not what a cache happens
//! to be showing right now.

use std::path::PathBuf;
use std::time::SystemTime;

use serde_json::{json, Value};

use reborn_core::manifest::Sha256Hex;
use reborn_core::net::UreqHttp;
use reborn_core::publish::upload::{self, Bucket, UploadRequest, Uploaded, UreqTransport};
use reborn_core::publish::vault::{r2_access_key, r2_secret_key};
use reborn_core::publish::{Event, Outcome, Sink};
use reborn_core::Platform;

use crate::ops;
use crate::settings::Settings;

/// Plan: what [`upload::plan_origin`] finds pending against the real
/// bucket, right now. Writes nothing and sends nothing.
#[tauri::command]
pub fn upload_plan() -> Result<Value, String> {
    let (platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let bucket = bucket_from(&platform, &settings)?;
    let public_base = settings.public_base.clone();

    let inputs = json!({"kind": "upload-plan"});

    ops::run_operation("upload:plan", true, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let transport = UreqTransport::new();
        let plan = upload::plan_origin(&origin, &|key| head_len(&transport, &bucket, key))?;

        if plan.is_empty() {
            let summary = format!("Nothing to upload: {public_base} already serves everything this origin holds.");
            sink(Event::out(summary.clone()));
            return Ok(Outcome::Done(json!({
                "summary": summary,
                "objects": 0,
                "bytes": 0,
                "items": Vec::<Value>::new(),
            })));
        }

        for item in plan.items() {
            sink(Event::out(format!("Pending       {} ({} byte(s))", item.key, item.bytes)));
        }
        let summary = format!(
            "{} object(s), {} byte(s) pending upload to {public_base}.",
            plan.len(),
            plan.total_bytes()
        );
        Ok(Outcome::Done(json!({
            "summary": summary,
            "objects": plan.len(),
            "bytes": plan.total_bytes(),
            "items": plan.items().map(|i| json!({"key": i.key, "bytes": i.bytes})).collect::<Vec<_>>(),
        })))
    })
    .map(|()| json!({"accepted": true}))
}

/// Run: upload everything [`upload_plan`] would find pending. `dry_run`
/// describes the plan and sends nothing, the same rule every other page's
/// Run step follows.
#[tauri::command]
pub fn upload_run(dry_run: bool) -> Result<Value, String> {
    let (platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let bucket = bucket_from(&platform, &settings)?;
    let public_base = settings.public_base.clone();

    let inputs = json!({"kind": "upload-run", "dry_run": dry_run});

    ops::run_operation("upload:run", dry_run, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let transport = UreqTransport::new();
        let plan = upload::plan_origin(&origin, &|key| head_len(&transport, &bucket, key))?;

        let http = UreqHttp::new(concat!("reborn-publisher/", env!("CARGO_PKG_VERSION")));
        let req = UploadRequest {
            bucket: &bucket,
            plan: &plan,
            multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD,
            part_size: upload::DEFAULT_PART_SIZE,
            attempts: upload::DEFAULT_ATTEMPTS,
            public_base: Some(&public_base),
            apply: !dry_run,
        };
        let uploaded = match upload::upload(&transport, &http, &req, sink)? {
            Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
            Outcome::Described => Uploaded::default(),
            Outcome::Done(u) => u,
        };

        let summary = if dry_run {
            format!(
                "Described: would upload {} object(s), {} byte(s) to {public_base}. Nothing was sent.",
                plan.len(),
                plan.total_bytes()
            )
        } else {
            format!("Uploaded {} object(s), {} byte(s) to {public_base}.", uploaded.objects, uploaded.bytes)
        };
        Ok(Outcome::Done(json!({
            "summary": summary,
            "uploaded": plan.items().map(|i| i.key.clone()).collect::<Vec<_>>(),
            "verified": uploaded.verified.first().cloned(),
        })))
    })
    .map(|()| json!({"accepted": true}))
}

// -- shared plumbing ------------------------------------------------------

fn settings_and_platform() -> Result<(Platform, Settings), String> {
    let platform = Platform::detect();
    let path = crate::settings::settings_path(&platform)?;
    let settings = Settings::load(&path)?;
    Ok((platform, settings))
}

fn origin_dir(settings: &Settings) -> PathBuf {
    PathBuf::from(&settings.origin)
}

/// The R2 bucket this app uploads to, from settings plus the vault. Never
/// built from an argument: see the module doc's Credentials section.
fn bucket_from(platform: &Platform, settings: &Settings) -> Result<Bucket, String> {
    let endpoint = non_empty(&settings.bucket_endpoint)
        .ok_or_else(|| "No bucket endpoint is configured. Set one in Settings before uploading.".to_string())?;
    let bucket_name = non_empty(&settings.bucket_name)
        .ok_or_else(|| "No bucket name is configured. Set one in Settings before uploading.".to_string())?;
    let access_key_id = r2_access_key(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 access key is stored in the vault. Store one on the Keys page before uploading.".to_string())?;
    let secret_access_key = r2_secret_key(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 secret key is stored in the vault. Store one on the Keys page before uploading.".to_string())?;
    Ok(Bucket { endpoint, bucket: bucket_name, region: "auto".to_string(), access_key_id, secret_access_key })
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// The length the bucket already serves `key` at, or `None` when it does
/// not have it (or answered anything other than 2xx). A signed HEAD, the
/// same request shape `plan_origin`'s own "already" callback expects.
fn head_len(transport: &dyn upload::Transport, bucket: &Bucket, key: &str) -> Option<u64> {
    let empty_sha = Sha256Hex::from_bytes(&[]);
    let path = upload::object_path(bucket, key);
    let req = upload::sign(bucket, "HEAD", &path, &[], &[], empty_sha.as_str(), SystemTime::now());
    let reply = transport.send(&req, upload::Body::Empty).ok()?;
    if !(200..300).contains(&reply.status) {
        return None;
    }
    reply.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.parse::<u64>().ok())
}
