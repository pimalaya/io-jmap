//! End-to-end JMAP tests against Fastmail.
//!
//! These tests require a Fastmail account and an app password:
//!
//! ```sh
//! FASTMAIL_API_TOKEN="<app-password-or-token>" \
//! FASTMAIL_EMAIL="user@fastmail.com" \
//! cargo test --test fastmail -- --include-ignored
//! ```
//!
//! `FASTMAIL_API_TOKEN` is the bare Fastmail app password or API
//! token; the test adds the `Bearer ` prefix itself.

mod common;

use std::env;

use io_http::rfc6750::bearer::HttpAuthBearer;

/// Full end-to-end JMAP test against Fastmail over HTTPS.
///
/// Exercises session discovery, mailbox CRUD, email
/// import/query/get/update, thread fetch, blob upload, and cleanup.
#[test]
#[ignore = "requires FASTMAIL_API_TOKEN + FASTMAIL_EMAIL env vars and --include-ignored"]
fn fastmail() {
    let email = env::var("FASTMAIL_EMAIL").expect("FASTMAIL_EMAIL not set");
    let token = env::var("FASTMAIL_API_TOKEN").expect("FASTMAIL_API_TOKEN not set");
    let token = HttpAuthBearer::new(token).to_authorization();

    common::run_jmaps("api.fastmail.com", 443, &token, &email);
}
