//! End-to-end JMAP tests against Fastmail.
//!
//! These tests require a Fastmail account and an app password:
//!
//! ```sh
//! FASTMAIL_BEARER_TOKEN=xxx \
//! cargo test --test fastmail -- --include-ignored
//! ```
//!
//! The `FASTMAIL_BEARER_TOKEN` value must be a full `Authorization` header
//! value, e.g. `"Bearer <app-password-or-token>"`.

use std::{env, net::TcpStream, sync::Arc};

use io_jmap::rfc8620::coroutines::session_get::{JmapSessionGet, JmapSessionGetResult};
use io_socket::runtimes::std_stream::handle;
use rustls::{ClientConfig, ClientConnection, StreamOwned, pki_types::ServerName};
use rustls_platform_verifier::ConfigVerifierExt;
use secrecy::SecretString;
use url::Url;

/// Fetch the JMAP session object from Fastmail.
///
/// # Example
///
/// ```sh
/// FASTMAIL_BEARER_TOKEN="Bearer xxx" \
/// cargo test --test fastmail -- --include-ignored
/// ```
#[test]
#[ignore = "requires FASTMAIL_BEARER_TOKEN env var and --include-ignored"]
fn fastmail_session_get() {
    let _ = env_logger::try_init();

    let token = env::var("FASTMAIL_BEARER_TOKEN").expect("FASTMAIL_BEARER_TOKEN not set");
    let token = SecretString::from(token);

    let host = "api.fastmail.com";
    let url = Url::parse(&format!("https://{host}/jmap/session/")).unwrap();

    let server_name = ServerName::try_from(host.to_owned()).expect("valid server name");
    let config = ClientConfig::with_platform_verifier().expect("TLS config");
    let conn = ClientConnection::new(Arc::new(config), server_name).expect("TLS handshake");
    let tcp = TcpStream::connect((host, 443u16)).expect("TCP connect");
    let mut stream = StreamOwned::new(conn, tcp);

    let mut coroutine = JmapSessionGet::new(&token, &url);
    let mut arg = None;

    let session = loop {
        match coroutine.resume(arg.take()) {
            JmapSessionGetResult::Ok { session, .. } => break session,
            JmapSessionGetResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
            JmapSessionGetResult::Redirect { url, .. } => {
                panic!("unexpected redirect to {url}")
            }
            JmapSessionGetResult::Err { err } => panic!("session get failed: {err}"),
        }
    };

    assert!(!session.username.is_empty(), "username should not be empty");
    assert!(
        !session.api_url.as_str().is_empty(),
        "apiUrl should not be empty"
    );

    println!("username: {}", session.username);
    println!("apiUrl:   {}", session.api_url);
    println!("primaryMailAccount: {}", session.primary_account_id());
}
