//! Tests for RFC 8621 — JMAP for Mail.
//!
//! All tests drive JMAP coroutines against pre-crafted in-memory HTTP
//! response buffers via [`stub::StubStream`].  No network connection is
//! made.

mod stub;

use io_jmap::{
    rfc8620::session::JmapSession,
    rfc8621::{
        email_get::{JmapEmailGet, JmapEmailGetResult},
        email_query::{JmapEmailQuery, JmapEmailQueryResult},
        mailbox::MailboxCreate,
        mailbox_get::{JmapMailboxGet, JmapMailboxGetResult},
        mailbox_query::{JmapMailboxQuery, JmapMailboxQueryResult},
        mailbox_set::{JmapMailboxSet, JmapMailboxSetArgs, JmapMailboxSetResult},
    },
};
use io_socket::runtimes::std_stream::handle;
use secrecy::SecretString;
use stub::StubStream;

// ── Session fixture ───────────────────────────────────────────────────────────

/// A minimal session object with a single mail account `acc1`.
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
    "urn:ietf:params:jmap:core": "acc1",
    "urn:ietf:params:jmap:mail": "acc1"
  },
  "capabilities": {
    "urn:ietf:params:jmap:core": {},
    "urn:ietf:params:jmap:mail": {}
  },
  "apiUrl": "http://example.com/jmap/api/",
  "downloadUrl": "http://example.com/jmap/download/{accountId}/{blobId}/{name}?accept={type}",
  "uploadUrl": "http://example.com/jmap/upload/{accountId}/",
  "eventSourceUrl": "http://example.com/jmap/eventsource/",
  "state": "s1"
}"#;

fn make_session() -> JmapSession {
    serde_json::from_slice(SESSION_JSON).expect("parse test session")
}

fn make_token() -> SecretString {
    SecretString::from("Bearer test-token")
}

// ── HTTP response helpers ─────────────────────────────────────────────────────

fn http_response(status: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

fn http_ok(body: &[u8]) -> Vec<u8> {
    http_response("200 OK", body)
}

// ── Coroutine drivers ─────────────────────────────────────────────────────────

fn run_mailbox_query(http_response_bytes: &[u8]) -> JmapMailboxQueryResult {
    let session = make_session();
    let token = make_token();
    let mut stream = StubStream::new(http_response_bytes);
    let mut coroutine =
        JmapMailboxQuery::new(&session, &token, None, None, None, None, None).unwrap();
    let mut arg = None;
    loop {
        match coroutine.resume(arg.take()) {
            JmapMailboxQueryResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
            any => return any,
        }
    }
}

fn run_mailbox_get(http_response_bytes: &[u8], ids: Option<Vec<String>>) -> JmapMailboxGetResult {
    let session = make_session();
    let token = make_token();
    let mut stream = StubStream::new(http_response_bytes);
    let mut coroutine = JmapMailboxGet::new(&session, &token, ids, None).unwrap();
    let mut arg = None;
    loop {
        match coroutine.resume(arg.take()) {
            JmapMailboxGetResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
            any => return any,
        }
    }
}

fn run_mailbox_set(http_response_bytes: &[u8], args: JmapMailboxSetArgs) -> JmapMailboxSetResult {
    let session = make_session();
    let token = make_token();
    let mut stream = StubStream::new(http_response_bytes);
    let mut coroutine = JmapMailboxSet::new(&session, &token, args).unwrap();
    let mut arg = None;
    loop {
        match coroutine.resume(arg.take()) {
            JmapMailboxSetResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
            any => return any,
        }
    }
}

fn run_email_query(http_response_bytes: &[u8]) -> JmapEmailQueryResult {
    let session = make_session();
    let token = make_token();
    let mut stream = StubStream::new(http_response_bytes);
    let mut coroutine =
        JmapEmailQuery::new(&session, &token, None, None, None, None, None).unwrap();
    let mut arg = None;
    loop {
        match coroutine.resume(arg.take()) {
            JmapEmailQueryResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
            any => return any,
        }
    }
}

fn run_email_get(http_response_bytes: &[u8], ids: Vec<String>) -> JmapEmailGetResult {
    let session = make_session();
    let token = make_token();
    let mut stream = StubStream::new(http_response_bytes);
    let mut coroutine = JmapEmailGet::new(&session, &token, ids, None, false, false, 0).unwrap();
    let mut arg = None;
    loop {
        match coroutine.resume(arg.take()) {
            JmapEmailGetResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
            any => return any,
        }
    }
}

// ── Mailbox/query tests ───────────────────────────────────────────────────────

#[test]
fn mailbox_query_ok() {
    let body = br#"{
      "methodResponses": [
        ["Mailbox/query", {"queryState": "s1", "position": 0, "ids": ["mbox1", "mbox2"]}, "c0"],
        ["Mailbox/get", {
          "state": "s1",
          "list": [
            {"id": "mbox1", "name": "Inbox"},
            {"id": "mbox2", "name": "Sent"}
          ],
          "notFound": []
        }, "c1"]
      ],
      "sessionState": "s1"
    }"#;

    match run_mailbox_query(&http_ok(body)) {
        JmapMailboxQueryResult::Ok { mailboxes, .. } => {
            assert_eq!(mailboxes.len(), 2, "expected 2 mailboxes");
            assert_eq!(mailboxes[0].id.as_deref(), Some("mbox1"));
            assert_eq!(mailboxes[0].name.as_deref(), Some("Inbox"));
            assert_eq!(mailboxes[1].id.as_deref(), Some("mbox2"));
            assert_eq!(mailboxes[1].name.as_deref(), Some("Sent"));
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn mailbox_query_empty() {
    let body = br#"{
      "methodResponses": [
        ["Mailbox/query", {"queryState": "s1", "position": 0, "ids": []}, "c0"],
        ["Mailbox/get", {"state": "s1", "list": [], "notFound": []}, "c1"]
      ],
      "sessionState": "s1"
    }"#;

    match run_mailbox_query(&http_ok(body)) {
        JmapMailboxQueryResult::Ok {
            mailboxes, total, ..
        } => {
            assert!(mailboxes.is_empty(), "expected no mailboxes");
            assert_eq!(total, None, "no total expected");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn mailbox_query_with_total() {
    let body = br#"{
      "methodResponses": [
        ["Mailbox/query", {"queryState": "s1", "position": 0, "total": 3, "ids": ["mbox1"]}, "c0"],
        ["Mailbox/get", {
          "state": "s1",
          "list": [{"id": "mbox1", "name": "Inbox"}],
          "notFound": []
        }, "c1"]
      ],
      "sessionState": "s1"
    }"#;

    match run_mailbox_query(&http_ok(body)) {
        JmapMailboxQueryResult::Ok {
            mailboxes,
            total,
            position,
            ..
        } => {
            assert_eq!(mailboxes.len(), 1);
            assert_eq!(total, Some(3), "total should be 3");
            assert_eq!(position, 0, "position should be 0");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn mailbox_query_method_error() {
    let body = br#"{
      "methodResponses": [
        ["error", {"type": "unknownMethod"}, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_mailbox_query(&http_ok(body)) {
        JmapMailboxQueryResult::Err { err } => {
            assert!(
                err.to_string().contains("unknownMethod"),
                "expected unknownMethod error, got: {err}"
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn mailbox_query_missing_get_response() {
    // Only Mailbox/query response; Mailbox/get is absent.
    let body = br#"{
      "methodResponses": [
        ["Mailbox/query", {"queryState": "s1", "position": 0, "ids": ["mbox1"]}, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_mailbox_query(&http_ok(body)) {
        JmapMailboxQueryResult::Err { err } => {
            assert!(
                err.to_string().contains("Missing"),
                "expected missing response error, got: {err}"
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn mailbox_query_http_error() {
    let response = http_response("401 Unauthorized", b"{}");

    match run_mailbox_query(&response) {
        JmapMailboxQueryResult::Err { err } => {
            assert!(
                err.to_string().contains("401"),
                "expected 401 error, got: {err}"
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

// ── Mailbox/get tests ─────────────────────────────────────────────────────────

#[test]
fn mailbox_get_ok() {
    let body = br#"{
      "methodResponses": [
        ["Mailbox/get", {
          "state": "s1",
          "list": [{"id": "mbox1", "name": "Inbox", "role": "inbox"}],
          "notFound": []
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_mailbox_get(&http_ok(body), Some(vec!["mbox1".to_owned()])) {
        JmapMailboxGetResult::Ok {
            mailboxes,
            not_found,
            new_state,
            ..
        } => {
            assert!(not_found.is_empty(), "expected no not_found");
            assert_eq!(new_state, "s1");
            assert_eq!(mailboxes.len(), 1);
            assert_eq!(mailboxes[0].id.as_deref(), Some("mbox1"));
            assert_eq!(mailboxes[0].name.as_deref(), Some("Inbox"));
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn mailbox_get_not_found() {
    let body = br#"{
      "methodResponses": [
        ["Mailbox/get", {
          "state": "s1",
          "list": [{"id": "mbox1", "name": "Inbox"}],
          "notFound": ["mbox-missing"]
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_mailbox_get(
        &http_ok(body),
        Some(vec!["mbox1".to_owned(), "mbox-missing".to_owned()]),
    ) {
        JmapMailboxGetResult::Ok {
            mailboxes,
            not_found,
            ..
        } => {
            assert_eq!(mailboxes.len(), 1, "expected 1 found mailbox");
            assert_eq!(not_found, vec!["mbox-missing"], "expected not_found");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn mailbox_get_all() {
    // ids = None fetches all mailboxes.
    let body = br#"{
      "methodResponses": [
        ["Mailbox/get", {
          "state": "s2",
          "list": [
            {"id": "mbox1", "name": "Inbox"},
            {"id": "mbox2", "name": "Drafts"},
            {"id": "mbox3", "name": "Sent"}
          ],
          "notFound": []
        }, "c0"]
      ],
      "sessionState": "s2"
    }"#;

    match run_mailbox_get(&http_ok(body), None) {
        JmapMailboxGetResult::Ok { mailboxes, .. } => {
            assert_eq!(mailboxes.len(), 3, "expected 3 mailboxes");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

// ── Mailbox/set tests ─────────────────────────────────────────────────────────

#[test]
fn mailbox_set_create_ok() {
    let body = br#"{
      "methodResponses": [
        ["Mailbox/set", {
          "newState": "s2",
          "created": {
            "new-mbox": {"id": "mbox-created", "name": "TestBox"}
          }
        }, "c0"]
      ],
      "sessionState": "s2"
    }"#;

    let mut create = std::collections::BTreeMap::new();
    create.insert(
        "new-mbox".to_owned(),
        MailboxCreate {
            name: Some("TestBox".to_owned()),
            ..Default::default()
        },
    );
    let args = JmapMailboxSetArgs {
        create: Some(create),
        ..Default::default()
    };

    match run_mailbox_set(&http_ok(body), args) {
        JmapMailboxSetResult::Ok {
            created,
            new_state,
            not_created,
            ..
        } => {
            assert_eq!(new_state, "s2");
            assert!(not_created.is_empty(), "expected no not_created");
            let mbox = created
                .get("new-mbox")
                .expect("created mailbox not in response");
            assert_eq!(
                mbox.id.as_deref(),
                Some("mbox-created"),
                "created mailbox id"
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn mailbox_set_destroy_ok() {
    let body = br#"{
      "methodResponses": [
        ["Mailbox/set", {
          "newState": "s3",
          "destroyed": ["mbox1"]
        }, "c0"]
      ],
      "sessionState": "s3"
    }"#;

    let args = JmapMailboxSetArgs {
        destroy: Some(vec!["mbox1".to_owned()]),
        ..Default::default()
    };

    match run_mailbox_set(&http_ok(body), args) {
        JmapMailboxSetResult::Ok {
            destroyed,
            new_state,
            not_destroyed,
            ..
        } => {
            assert_eq!(new_state, "s3");
            assert!(not_destroyed.is_empty(), "expected no not_destroyed");
            assert_eq!(destroyed, vec!["mbox1".to_owned()]);
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

// ── Email/query tests ─────────────────────────────────────────────────────────

#[test]
fn email_query_ok() {
    let body = br#"{
      "methodResponses": [
        ["Email/query", {"queryState": "s1", "position": 0, "ids": ["email1", "email2"]}, "c0"],
        ["Email/get", {
          "state": "s1",
          "list": [
            {"id": "email1", "threadId": "thread1", "subject": "Hello"},
            {"id": "email2", "threadId": "thread2", "subject": "World"}
          ],
          "notFound": []
        }, "c1"]
      ],
      "sessionState": "s1"
    }"#;

    match run_email_query(&http_ok(body)) {
        JmapEmailQueryResult::Ok { emails, .. } => {
            assert_eq!(emails.len(), 2, "expected 2 emails");
            assert_eq!(emails[0].id.as_deref(), Some("email1"));
            assert_eq!(emails[0].thread_id.as_deref(), Some("thread1"));
            assert_eq!(emails[0].subject.as_deref(), Some("Hello"));
            assert_eq!(emails[1].id.as_deref(), Some("email2"));
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn email_query_empty() {
    let body = br#"{
      "methodResponses": [
        ["Email/query", {"queryState": "s1", "position": 0, "ids": []}, "c0"],
        ["Email/get", {"state": "s1", "list": [], "notFound": []}, "c1"]
      ],
      "sessionState": "s1"
    }"#;

    match run_email_query(&http_ok(body)) {
        JmapEmailQueryResult::Ok { emails, .. } => {
            assert!(emails.is_empty(), "expected no emails");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn email_query_method_error() {
    let body = br#"{
      "methodResponses": [
        ["error", {"type": "invalidArguments", "description": "bad filter"}, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_email_query(&http_ok(body)) {
        JmapEmailQueryResult::Err { err } => {
            assert!(
                err.to_string().contains("invalidArguments"),
                "expected invalidArguments error, got: {err}"
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

// ── Email/get tests ───────────────────────────────────────────────────────────

#[test]
fn email_get_ok() {
    let body = br#"{
      "methodResponses": [
        ["Email/get", {
          "state": "s1",
          "list": [{"id": "email1", "subject": "Integration test", "threadId": "thread1"}],
          "notFound": []
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_email_get(&http_ok(body), vec!["email1".to_owned()]) {
        JmapEmailGetResult::Ok {
            emails,
            not_found,
            new_state,
            ..
        } => {
            assert!(not_found.is_empty(), "expected no not_found");
            assert_eq!(new_state, "s1");
            assert_eq!(emails.len(), 1);
            assert_eq!(emails[0].id.as_deref(), Some("email1"));
            assert_eq!(emails[0].subject.as_deref(), Some("Integration test"));
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn email_get_not_found() {
    let body = br#"{
      "methodResponses": [
        ["Email/get", {
          "state": "s1",
          "list": [],
          "notFound": ["email-missing"]
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_email_get(&http_ok(body), vec!["email-missing".to_owned()]) {
        JmapEmailGetResult::Ok {
            emails, not_found, ..
        } => {
            assert!(emails.is_empty(), "expected no emails in list");
            assert_eq!(not_found, vec!["email-missing"]);
        }
        other => panic!("unexpected result: {other:?}"),
    }
}
