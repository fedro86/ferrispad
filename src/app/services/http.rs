//! Shared blocking HTTPS client: `ureq` with rustls on the `ring` provider and
//! the bundled webpki root store (no OpenSSL, no aws-lc, no OS cert store).

use std::time::Duration;

use ureq::RequestBuilder;
use ureq::typestate::WithoutBody;

/// User-Agent sent on every request (GitHub's API rejects requests without one).
pub const USER_AGENT: &str = "FerrisPad";

/// A GET request with a whole-request `timeout_secs` deadline that refuses
/// plain `http://` (including redirects to it). HTTP error statuses come back
/// as ordinary responses so callers can map them themselves.
pub fn get(url: &str, timeout_secs: u64) -> RequestBuilder<WithoutBody> {
    build(url, timeout_secs, true)
}

/// Like [`get`] but allows plain HTTP — for loopback test servers only.
#[cfg(test)]
pub(crate) fn get_allow_http(url: &str, timeout_secs: u64) -> RequestBuilder<WithoutBody> {
    build(url, timeout_secs, false)
}

fn build(url: &str, timeout_secs: u64, https_only: bool) -> RequestBuilder<WithoutBody> {
    ureq::get(url)
        .config()
        .https_only(https_only)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .build()
        .header("User-Agent", USER_AGENT)
}
