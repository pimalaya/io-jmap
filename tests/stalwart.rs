use io_http::rfc7617::basic::HttpAuthBasic;

mod common;

/// End-to-end JMAP tests against a local Stalwart server.
///
/// Start a local Stalwart instance and run with:
///
/// ```sh
/// ./tests/stalwart.sh
/// cargo test --test stalwart -- --include-ignored
/// ```
///
/// The bootstrap script provisions one domain (`pimalaya.org`) and one
/// user (`test@pimalaya.org`) with a strong password (Stalwart enforces
/// a zxcvbn-style strength check). Stalwart listens on port 8080 for
/// JMAP sessions (plain HTTP).
#[test]
#[ignore = "requires a running Stalwart instance on localhost:8080 and --include-ignored"]
fn stalwart() {
    let creds = HttpAuthBasic::new("test@pimalaya.org", "P!malaya-test-2026").to_authorization();
    common::run_jmap("localhost", 8080, &creds, "test@pimalaya.org");
}
