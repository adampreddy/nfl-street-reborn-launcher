//! The secrets the publisher needs, kept in the platform credential vault so
//! none of them ever sits on a command line or in a file.

use crate::{Platform, Result};

/// The vault account the site's API token is kept under.
pub const SITE_TOKEN_ACCOUNT: &str = "nsr.site.export";

pub fn site_token(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(SITE_TOKEN_ACCOUNT)
}

pub fn store_site_token(platform: &Platform, token: &str) -> Result<()> {
    platform.credentials.store(SITE_TOKEN_ACCOUNT, token)
}

pub fn forget_site_token(platform: &Platform) -> Result<()> {
    platform.credentials.delete(SITE_TOKEN_ACCOUNT)
}
