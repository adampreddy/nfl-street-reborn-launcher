//! The secrets the publisher needs, kept in the platform credential vault so
//! none of them ever sits on a command line or in a file.

use crate::{Platform, Result};

/// The vault account the site's API token is kept under.
pub const SITE_TOKEN_ACCOUNT: &str = "nsr.site.export";
/// The vault account the R2 access key id is kept under.
pub const R2_ACCESS_KEY_ACCOUNT: &str = "nsr.publish.r2_access_key_id";
/// The vault account the R2 secret access key is kept under.
pub const R2_SECRET_KEY_ACCOUNT: &str = "nsr.publish.r2_secret_access_key";

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
pub fn r2_access_key(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(R2_ACCESS_KEY_ACCOUNT)
}

pub fn store_r2_access_key(platform: &Platform, value: &str) -> Result<()> {
    platform.credentials.store(R2_ACCESS_KEY_ACCOUNT, value)
}

pub fn forget_r2_access_key(platform: &Platform) -> Result<()> {
    platform.credentials.delete(R2_ACCESS_KEY_ACCOUNT)
}

/// The bucket's R2 secret access key. Whoever holds this can publish to
/// every launcher that trusts this bucket; treated with the same care as
/// the site token and never read back once stored.
pub fn r2_secret_key(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(R2_SECRET_KEY_ACCOUNT)
}

pub fn store_r2_secret_key(platform: &Platform, value: &str) -> Result<()> {
    platform.credentials.store(R2_SECRET_KEY_ACCOUNT, value)
}

pub fn forget_r2_secret_key(platform: &Platform) -> Result<()> {
    platform.credentials.delete(R2_SECRET_KEY_ACCOUNT)
}
