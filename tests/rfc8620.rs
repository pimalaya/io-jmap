//! Tests for RFC 8620 — JSON Meta Application Protocol.
//!
//! All tests drive JMAP coroutines against pre-crafted in-memory HTTP
//! response buffers. No network connection is made.

use io_jmap::rfc8620::session_get::{JmapSessionGet, JmapSessionGetResult};
use secrecy::SecretString;
use url::Url;

const SESSION_JSON: &[u8] = br#"{
  "username": "user@example.com",
  "accounts": {
    "acc1": {
      "name": "Test Account",
      "isPersonal": true,
      "isReadOnly": false,
      "accountCapabilities": {}
    }
  },
  "primaryAccounts": {
    "urn:ietf:params:jmap:mail": "acc1"
  },
  "capabilities": {
    "urn:ietf:params:jmap:core": {},
    "urn:ietf:params:jmap:mail": {}
  },
  "apiUrl": "https://example.com/jmap/api/",
  "downloadUrl": "https://example.com/jmap/download/{accountId}/{blobId}/{name}?accept={type}",
  "uploadUrl": "https://example.com/jmap/upload/{accountId}/",
  "eventSourceUrl": "https://example.com/jmap/eventsource/?types={types}&closeafter={closeafter}&ping={ping}",
  "state": "s1"
}"#;

fn http_response(status: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

fn run_session_get(http_response_bytes: &[u8]) -> JmapSessionGetResult {
    let token = SecretString::from("Bearer test-token");
    let url = Url::parse("http://example.com/jmap/session/").unwrap();
    let mut coroutine = JmapSessionGet::new(&token, &url);
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapSessionGetResult::WantsWrite(_) => arg = None,
            JmapSessionGetResult::WantsRead => arg = Some(http_response_bytes),
            any => return any,
        }
    }
}

#[test]
fn session_get_200() {
    let response = http_response("200 OK", SESSION_JSON);

    match run_session_get(&response) {
        JmapSessionGetResult::Ok { session, .. } => {
            assert_eq!(session.username, "user@example.com");
            assert_eq!(
                session.primary_account_id_for("urn:ietf:params:jmap:mail"),
                "acc1"
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn session_get_401() {
    let response = http_response("401 Unauthorized", b"{}");

    match run_session_get(&response) {
        JmapSessionGetResult::Err(err) => {
            assert!(err.to_string().contains("401"));
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn session_get_invalid_json() {
    let response = http_response("200 OK", b"not-json");

    match run_session_get(&response) {
        JmapSessionGetResult::Err(_) => {}
        other => panic!("expected parse error, got: {other:?}"),
    }
}

#[test]
fn session_get_redirect() {
    let location = "http://api.example.com/jmap/session/";
    let body = b"Moved";
    let mut full = format!(
        "HTTP/1.1 301 Moved Permanently\r\nLocation: {location}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    full.extend_from_slice(body);

    match run_session_get(&full) {
        JmapSessionGetResult::WantsRedirect { url, .. } => {
            assert_eq!(url.as_str(), location);
        }
        other => panic!("unexpected result: {other:?}"),
    }
}
