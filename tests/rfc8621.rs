//! Tests for RFC 8621: JMAP for Mail.
//!
//! All tests resume JMAP coroutines against pre-crafted in-memory HTTP
//! response buffers. No network connection is made.

use std::collections::BTreeMap;

use io_jmap::{
    coroutine::*,
    rfc8620::{
        error::JmapMethodError,
        session::{JmapSession, JmapSessionAccountError},
    },
    rfc8621::{
        email::{
            copy::*,
            get::{JmapEmailGet, JmapEmailGetError, JmapEmailGetOptions, JmapEmailGetOutput},
            import::*,
            parse::*,
            query::{
                JmapEmailComparator, JmapEmailQuery, JmapEmailQueryError, JmapEmailQueryOptions,
                JmapEmailQueryOutput,
            },
            set::*,
        },
        email_submission::{JmapEmailSubmissionSetItemError, cancel::*, set::*},
        identity::set::JmapIdentityUpdate,
        mailbox::{
            get::{
                JmapMailboxGet, JmapMailboxGetError, JmapMailboxGetOptions, JmapMailboxGetOutput,
            },
            query::{
                JmapMailboxQuery, JmapMailboxQueryError, JmapMailboxQueryOptions,
                JmapMailboxQueryOutput,
            },
            set::{
                JmapMailboxCreate, JmapMailboxSet, JmapMailboxSetArgs, JmapMailboxSetError,
                JmapMailboxSetOutput, JmapMailboxUpdate,
            },
        },
        vacation_response::get::{JmapVacationResponseGet, JmapVacationResponseGetError},
    },
};
use secrecy::SecretString;
use serde_json::Value;

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

fn run_mailbox_query(
    http_response_bytes: &[u8],
) -> JmapCoroutineState<JmapYield, Result<JmapMailboxQueryOutput, JmapMailboxQueryError>> {
    let session = make_session();
    let token = make_token();
    let mut coroutine =
        JmapMailboxQuery::new(&session, &token, JmapMailboxQueryOptions::default()).unwrap();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(_)) => arg = None,
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return any,
        }
    }
}

fn run_mailbox_get(
    http_response_bytes: &[u8],
    ids: Option<Vec<String>>,
) -> JmapCoroutineState<JmapYield, Result<JmapMailboxGetOutput, JmapMailboxGetError>> {
    let session = make_session();
    let token = make_token();
    let mut coroutine = JmapMailboxGet::new(
        &session,
        &token,
        JmapMailboxGetOptions {
            ids,
            ..Default::default()
        },
    )
    .unwrap();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(_)) => arg = None,
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return any,
        }
    }
}

fn run_mailbox_set(
    http_response_bytes: &[u8],
    args: JmapMailboxSetArgs,
) -> JmapCoroutineState<JmapYield, Result<JmapMailboxSetOutput, JmapMailboxSetError>> {
    let session = make_session();
    let token = make_token();
    let mut coroutine = JmapMailboxSet::new(&session, &token, args).unwrap();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(_)) => arg = None,
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return any,
        }
    }
}

fn run_email_query(
    http_response_bytes: &[u8],
) -> JmapCoroutineState<JmapYield, Result<JmapEmailQueryOutput, JmapEmailQueryError>> {
    let session = make_session();
    let token = make_token();
    let mut coroutine =
        JmapEmailQuery::new(&session, &token, JmapEmailQueryOptions::default()).unwrap();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(_)) => arg = None,
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return any,
        }
    }
}

fn run_email_get(
    http_response_bytes: &[u8],
    ids: Vec<String>,
) -> JmapCoroutineState<JmapYield, Result<JmapEmailGetOutput, JmapEmailGetError>> {
    let session = make_session();
    let token = make_token();
    let mut coroutine =
        JmapEmailGet::new(&session, &token, ids, JmapEmailGetOptions::default()).unwrap();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(_)) => arg = None,
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return any,
        }
    }
}

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
        JmapCoroutineState::Complete(Ok(JmapMailboxQueryOutput { mailboxes, .. })) => {
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
fn mailbox_query_keeps_the_query_order() {
    let body = br#"{
      "methodResponses": [
        ["Mailbox/query", {"queryState": "s1", "position": 0, "ids": ["m3", "m1", "m2"]}, "c0"],
        ["Mailbox/get", {
          "state": "s1",
          "list": [{"id": "m1"}, {"id": "m2"}, {"id": "m3"}],
          "notFound": []
        }, "c1"]
      ],
      "sessionState": "s1"
    }"#;

    let JmapCoroutineState::Complete(Ok(out)) = run_mailbox_query(&http_ok(body)) else {
        panic!("the query did not complete");
    };
    let ids: Vec<_> = out.mailboxes.iter().map(|m| m.id.as_deref()).collect();
    assert_eq!(ids, [Some("m3"), Some("m1"), Some("m2")]);
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
        JmapCoroutineState::Complete(Ok(JmapMailboxQueryOutput {
            mailboxes, total, ..
        })) => {
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
        JmapCoroutineState::Complete(Ok(JmapMailboxQueryOutput {
            mailboxes,
            total,
            position,
            ..
        })) => {
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
        JmapCoroutineState::Complete(Err(err)) => {
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
    let body = br#"{
      "methodResponses": [
        ["Mailbox/query", {"queryState": "s1", "position": 0, "ids": ["mbox1"]}, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_mailbox_query(&http_ok(body)) {
        JmapCoroutineState::Complete(Err(err)) => {
            assert!(
                err.to_string().contains("missing"),
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
        JmapCoroutineState::Complete(Err(err)) => {
            assert!(
                err.to_string().contains("401"),
                "expected 401 error, got: {err}"
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

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
        JmapCoroutineState::Complete(Ok(JmapMailboxGetOutput {
            mailboxes,
            not_found,
            new_state,
            ..
        })) => {
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
        JmapCoroutineState::Complete(Ok(JmapMailboxGetOutput {
            mailboxes,
            not_found,
            ..
        })) => {
            assert_eq!(mailboxes.len(), 1, "expected 1 found mailbox");
            assert_eq!(not_found, vec!["mbox-missing"], "expected not_found");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn mailbox_get_all() {
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
        JmapCoroutineState::Complete(Ok(JmapMailboxGetOutput { mailboxes, .. })) => {
            assert_eq!(mailboxes.len(), 3, "expected 3 mailboxes");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

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
        JmapMailboxCreate {
            name: Some("TestBox".to_owned()),
            ..Default::default()
        },
    );
    let args = JmapMailboxSetArgs {
        create: Some(create),
        ..Default::default()
    };

    match run_mailbox_set(&http_ok(body), args) {
        JmapCoroutineState::Complete(Ok(JmapMailboxSetOutput {
            created,
            new_state,
            not_created,
            ..
        })) => {
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
        JmapCoroutineState::Complete(Ok(JmapMailboxSetOutput {
            destroyed,
            new_state,
            not_destroyed,
            ..
        })) => {
            assert_eq!(new_state, "s3");
            assert!(not_destroyed.is_empty(), "expected no not_destroyed");
            assert_eq!(destroyed, vec!["mbox1".to_owned()]);
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

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
        JmapCoroutineState::Complete(Ok(JmapEmailQueryOutput { emails, .. })) => {
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
fn email_query_keeps_the_query_order() {
    let body = br#"{
      "methodResponses": [
        ["Email/query", {"queryState": "s1", "position": 0, "ids": ["e2", "e1"]}, "c0"],
        ["Email/get", {
          "state": "s1",
          "list": [{"id": "e1"}, {"id": "e2"}],
          "notFound": []
        }, "c1"]
      ],
      "sessionState": "s1"
    }"#;

    let JmapCoroutineState::Complete(Ok(out)) = run_email_query(&http_ok(body)) else {
        panic!("the query did not complete");
    };
    let ids: Vec<_> = out.emails.iter().map(|e| e.id.as_deref()).collect();
    assert_eq!(ids, [Some("e2"), Some("e1")]);
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
        JmapCoroutineState::Complete(Ok(JmapEmailQueryOutput { emails, .. })) => {
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
        JmapCoroutineState::Complete(Err(err)) => {
            assert!(
                err.to_string().contains("invalidArguments"),
                "expected invalidArguments error, got: {err}"
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn email_query_anchor_not_found() {
    let body = br#"{
      "methodResponses": [
        ["error", {"type": "anchorNotFound"}, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_email_query(&http_ok(body)) {
        JmapCoroutineState::Complete(Err(JmapEmailQueryError::QueryMethod(
            JmapMethodError::AnchorNotFound { .. },
        ))) => {}
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn email_query_unsupported_sort() {
    let body = br#"{
      "methodResponses": [
        ["error", {"type": "unsupportedSort", "description": "size"}, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_email_query(&http_ok(body)) {
        JmapCoroutineState::Complete(Err(JmapEmailQueryError::QueryMethod(
            err @ JmapMethodError::UnsupportedSort { .. },
        ))) => assert_eq!(err.to_string(), "JMAP unsupportedSort: size"),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn email_query_default_args() {
    let args = email_query_args(JmapEmailQueryOptions::default());

    assert_eq!(args["calculateTotal"], true);
    assert!(args.get("anchor").is_none());
    assert!(args.get("anchorOffset").is_none());
    assert!(args.get("collapseThreads").is_none());
}

#[test]
fn email_query_window_args() {
    let args = email_query_args(JmapEmailQueryOptions {
        sort: Some(vec![JmapEmailComparator::sent_at_desc()]),
        anchor: Some("email1".into()),
        anchor_offset: Some(-5),
        limit: Some(20),
        calculate_total: false,
        collapse_threads: true,
        ..Default::default()
    });

    assert_eq!(args["sort"][0]["property"], "sentAt");
    assert_eq!(args["sort"][0]["isAscending"], false);
    assert_eq!(args["anchor"], "email1");
    assert_eq!(args["anchorOffset"], -5);
    assert_eq!(args["limit"], 20);
    assert_eq!(args["calculateTotal"], false);
    assert_eq!(args["collapseThreads"], true);
}

/// Returns the `Email/query` arguments of the request the coroutine
/// writes.
fn email_query_args(opts: JmapEmailQueryOptions) -> Value {
    let session = make_session();
    let token = make_token();
    let mut coroutine = JmapEmailQuery::new(&session, &token, opts).unwrap();

    let JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) = coroutine.resume(None) else {
        panic!("expected WantsWrite");
    };

    let start = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("HTTP head end")
        + 4;
    let request: Value = serde_json::from_slice(&bytes[start..]).unwrap();

    assert_eq!(request["methodCalls"][0][0], "Email/query");
    request["methodCalls"][0][1].clone()
}

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
        JmapCoroutineState::Complete(Ok(JmapEmailGetOutput {
            emails,
            not_found,
            new_state,
            ..
        })) => {
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
        JmapCoroutineState::Complete(Ok(JmapEmailGetOutput {
            emails, not_found, ..
        })) => {
            assert!(emails.is_empty(), "expected no emails in list");
            assert_eq!(not_found, vec!["email-missing"]);
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

/// Resumes `coroutine` to completion, answering its one read with `reply`.
fn complete<C: JmapCoroutine<Yield = JmapYield>>(mut coroutine: C, reply: &[u8]) -> C::Return {
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(_)) => {}
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(reply),
            JmapCoroutineState::Complete(out) => return out,
        }
    }
}

#[test]
fn constructor_without_a_primary_account_fails() {
    let mut session = make_session();
    let opts = JmapEmailGetOptions::default;

    session.primary_accounts.clear();
    assert!(matches!(
        JmapEmailGet::new(&session, &make_token(), Vec::new(), opts()),
        Err(JmapEmailGetError::Account(JmapSessionAccountError(capability)))
            if capability == "urn:ietf:params:jmap:mail"
    ));
    assert!(matches!(
        JmapVacationResponseGet::new(&session, &make_token()),
        Err(JmapVacationResponseGetError::Account(_))
    ));

    let mail = "urn:ietf:params:jmap:mail".to_owned();
    session.primary_accounts.insert(mail, String::new());
    assert!(matches!(
        JmapEmailGet::new(&session, &make_token(), Vec::new(), opts()),
        Err(JmapEmailGetError::Account(_))
    ));
}

#[test]
fn vacation_response_get_falls_back_to_the_mail_account() {
    let coroutine = JmapVacationResponseGet::new(&make_session(), &make_token());
    assert!(coroutine.is_ok());
}

fn submission_create() -> BTreeMap<String, JmapEmailSubmissionCreate> {
    let create = JmapEmailSubmissionCreate {
        identity_id: "i1".to_owned(),
        email_id: "e1".to_owned(),
        envelope: None,
    };

    BTreeMap::from([("c1".to_owned(), create)])
}

#[test]
fn email_submission_set_refusal_without_new_state() {
    let body = br#"{
      "methodResponses": [
        ["EmailSubmission/set", {
          "accountId": "acc1",
          "notCreated": {"c1": {"type": "noRecipients"}}
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    let coroutine =
        JmapEmailSubmissionSet::new(&make_session(), &make_token(), submission_create()).unwrap();
    let out = complete(coroutine, &http_ok(body)).expect("refusal read");

    assert_eq!(out.new_state, "");
    assert!(matches!(
        out.not_created["c1"],
        JmapEmailSubmissionSetItemError::NoRecipients { .. }
    ));
}

#[test]
fn email_submission_set_reads_the_implicit_email_set() {
    let body = br#"{
      "methodResponses": [
        ["EmailSubmission/set", {
          "accountId": "acc1",
          "newState": "s2",
          "created": {"c1": {"id": "S1"}}
        }, "c0"],
        ["Email/set", {
          "accountId": "acc1",
          "newState": "e2",
          "notUpdated": {"e1": {"type": "notFound"}}
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    let patch = JmapEmailPatch::default().add_to_mailbox("sent");
    let args = JmapEmailSubmissionSetArgs {
        create: submission_create(),
        on_success_update_email: Some(BTreeMap::from([("#c1".to_owned(), patch)])),
        on_success_destroy_email: None,
    };
    let coroutine = JmapEmailSubmissionSet::new(&make_session(), &make_token(), args).unwrap();
    let out = complete(coroutine, &http_ok(body)).expect("submission read");

    assert!(out.created.contains_key("c1"));

    let email_set = out.email_set.expect("implicit response").expect("no error");
    assert_eq!(email_set.new_state, "e2");
    assert!(matches!(
        email_set.not_updated["e1"],
        JmapEmailSetItemError::NotFound { .. }
    ));
}

#[test]
fn email_submission_set_keeps_a_failed_implicit_email_set() {
    let body = br#"{
      "methodResponses": [
        ["EmailSubmission/set", {
          "accountId": "acc1",
          "newState": "s2",
          "created": {"c1": {"id": "S1"}}
        }, "c0"],
        ["error", {"type": "serverFail", "description": "down"}, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    let args = JmapEmailSubmissionSetArgs {
        create: submission_create(),
        on_success_update_email: None,
        on_success_destroy_email: Some(vec!["#c1".to_owned()]),
    };
    let coroutine = JmapEmailSubmissionSet::new(&make_session(), &make_token(), args).unwrap();
    let out = complete(coroutine, &http_ok(body)).expect("submission read");

    assert!(out.created.contains_key("c1"));
    assert!(matches!(
        out.email_set,
        Some(Err(JmapMethodError::ServerFail { .. }))
    ));
}

#[test]
fn email_submission_cancel_refusal_with_null_new_state() {
    let body = br#"{
      "methodResponses": [
        ["EmailSubmission/set", {
          "accountId": "acc1",
          "newState": null,
          "notUpdated": {"S1": {"type": "cannotUnsend"}}
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    let ids = vec!["S1".to_owned()];
    let coroutine = JmapEmailSubmissionCancel::new(&make_session(), &make_token(), ids).unwrap();
    let out = complete(coroutine, &http_ok(body)).expect("refusal read");

    assert_eq!(out.new_state, "");
    assert!(matches!(
        out.not_updated["S1"],
        JmapEmailSubmissionSetItemError::CannotUnsendMessage { .. }
    ));
}

#[test]
fn email_import_refusal_without_new_state() {
    let body = br#"{
      "methodResponses": [
        ["Email/import", {
          "accountId": "acc1",
          "notCreated": {"m0": {"type": "invalidEmail"}}
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    let import = JmapEmailImportArgs {
        blob_id: "b1".to_owned(),
        mailbox_ids: BTreeMap::from([("drafts".to_owned(), true)]),
        keywords: None,
        received_at: None,
    };
    let emails = BTreeMap::from([("m0".to_owned(), import)]);
    let coroutine = JmapEmailImport::new(&make_session(), &make_token(), emails).unwrap();
    let out = complete(coroutine, &http_ok(body)).expect("refusal read");

    assert_eq!(out.new_state, "");
    assert!(out.not_created.contains_key("m0"));
}

#[test]
fn email_copy_refusal_without_new_state() {
    let body = br#"{
      "methodResponses": [
        ["Email/copy", {
          "fromAccountId": "acc0",
          "accountId": "acc1",
          "notCreated": {"m0": {"type": "notFound"}}
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    let copy = JmapEmailCopyArgs {
        id: "e1".to_owned(),
        mailbox_ids: BTreeMap::from([("inbox".to_owned(), true)]),
        keywords: None,
        received_at: None,
    };
    let emails = BTreeMap::from([("m0".to_owned(), copy)]);
    let coroutine = JmapEmailCopy::new(&make_session(), &make_token(), "acc0", emails).unwrap();
    let out = complete(coroutine, &http_ok(body)).expect("refusal read");

    assert_eq!(out.new_state, "");
    assert!(matches!(
        out.not_created["m0"],
        JmapEmailCopyItemError::NotFound { .. }
    ));
}

#[test]
fn email_parse_reads_a_null_parsed() {
    let body = br#"{
      "methodResponses": [
        ["Email/parse", {
          "accountId": "acc1",
          "parsed": null,
          "notParsable": ["b1"],
          "notFound": ["b2"]
        }, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    let blob_ids = vec!["b1".to_owned(), "b2".to_owned()];
    let opts = JmapEmailParseOptions::default();
    let coroutine = JmapEmailParse::new(&make_session(), &make_token(), blob_ids, opts).unwrap();
    let out = complete(coroutine, &http_ok(body)).expect("lists read");

    assert!(out.parsed.is_empty());
    assert_eq!(out.not_parsable, vec!["b1"]);
    assert_eq!(out.not_found, vec!["b2"]);
}

#[test]
fn mailbox_update_clears_a_nullable_property() {
    let patch = JmapMailboxUpdate {
        parent_id: Some(None),
        role: Some(None),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(patch).unwrap(),
        serde_json::json!({ "parentId": null, "role": null })
    );

    let patch = JmapMailboxUpdate {
        parent_id: Some(Some("m1".to_owned())),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(patch).unwrap(),
        serde_json::json!({ "parentId": "m1" })
    );
}

#[test]
fn identity_update_clears_a_nullable_property() {
    let patch = JmapIdentityUpdate {
        reply_to: Some(None),
        bcc: Some(None),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(patch).unwrap(),
        serde_json::json!({ "replyTo": null, "bcc": null })
    );
}
