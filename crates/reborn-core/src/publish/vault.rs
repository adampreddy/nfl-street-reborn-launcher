//! The secrets the publisher needs, kept in the platform credential vault so
//! none of them ever sits on a command line or in a file.

use crate::publish::upload::Bucket;
use crate::{Platform, Result};

/// The vault account the site's API token is kept under.
pub const SITE_TOKEN_ACCOUNT: &str = "nsr.site.export";
/// The vault account the R2 access key id is kept under.
pub const R2_ACCESS_ACCOUNT: &str = "nsr.r2.access";
/// The vault account the R2 secret access key is kept under.
pub const R2_SECRET_ACCOUNT: &str = "nsr.r2.secret";

pub fn site_token(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(SITE_TOKEN_ACCOUNT)
}

pub fn store_site_token(platform: &Platform, token: &str) -> Result<()> {
    platform.credentials.store(SITE_TOKEN_ACCOUNT, token)
}

pub fn forget_site_token(platform: &Platform) -> Result<()> {
    platform.credentials.delete(SITE_TOKEN_ACCOUNT)
}

/// The bucket's R2 access key id, the everyday-safe half of its write
/// credentials.
pub fn r2_access(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(R2_ACCESS_ACCOUNT)
}

pub fn store_r2_access(platform: &Platform, value: &str) -> Result<()> {
    platform.credentials.store(R2_ACCESS_ACCOUNT, value)
}

pub fn forget_r2_access(platform: &Platform) -> Result<()> {
    platform.credentials.delete(R2_ACCESS_ACCOUNT)
}

/// The bucket's R2 secret access key. Whoever holds this can publish to
/// every launcher that trusts this bucket; treated with the same care as
/// the site token and never read back once stored.
pub fn r2_secret(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(R2_SECRET_ACCOUNT)
}

pub fn store_r2_secret(platform: &Platform, value: &str) -> Result<()> {
    platform.credentials.store(R2_SECRET_ACCOUNT, value)
}

pub fn forget_r2_secret(platform: &Platform) -> Result<()> {
    platform.credentials.delete(R2_SECRET_ACCOUNT)
}

/// The production bucket's non-secret address: Cloudflare R2, bucket
/// `reborn-content` (see `docs/NEXT_STEPS.md`, and `docs/RUNNING_UPDATES.md`
/// for how it was provisioned). R2 does not use AWS regions; its
/// S3-compatible API takes the literal string "auto" in their place.
pub const PRODUCTION_ENDPOINT: &str = "https://80b77957d6f66ab8e416ae7618252b07.r2.cloudflarestorage.com";
pub const PRODUCTION_BUCKET: &str = "reborn-content";
pub const PRODUCTION_REGION: &str = "auto";

/// The bucket a plain `rebornctl publish upload` writes to: the production
/// address above, with the write credentials read from the vault. `None`
/// when either credential half has not been stored yet, so a caller can say
/// that plainly instead of signing a request with an empty secret.
///
/// A caller free to point at a different endpoint for a test overrides
/// `endpoint` and `bucket` on the value this returns (`rebornctl publish
/// upload --endpoint --bucket`); the credentials and the region travel
/// unchanged either way, since only the address of the stand-in differs, not
/// who is allowed to write to it.
pub fn bucket(platform: &Platform) -> Result<Option<Bucket>> {
    let (Some(access_key_id), Some(secret_access_key)) = (r2_access(platform)?, r2_secret(platform)?) else {
        return Ok(None);
    };
    Ok(Some(Bucket {
        endpoint: PRODUCTION_ENDPOINT.to_string(),
        bucket: PRODUCTION_BUCKET.to_string(),
        region: PRODUCTION_REGION.to_string(),
        access_key_id,
        secret_access_key,
    }))
}
