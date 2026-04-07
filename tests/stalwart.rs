use io_http::rfc7617::basic::BasicCredentials;

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
/// The test uses the credentials created by the setup script:
/// - username: `test`
/// - password: `test`
///
/// Stalwart listens on port 8080 for JMAP sessions (plain HTTP).
#[test]
#[ignore = "requires a running Stalwart instance on localhost:8080 and --include-ignored"]
fn stalwart() {
    let creds = BasicCredentials::new("test", "test").to_authorization();
    common::run_jmap("localhost", 8080, &creds, "test@pimalaya.org");
}
