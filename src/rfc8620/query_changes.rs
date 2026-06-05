//! Generic JMAP `Foo/queryChanges` coroutine (RFC 8620 §5.6): wraps
//! [`JmapSend`] with a `since_query_state` cursor and decodes the removed/added
//! id deltas.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::rfc8620::query_changes::{JmapQueryChanges, JmapQueryChangesOptions};
//! use secrecy::SecretString;
//! use serde_json::Value;
//! use url::Url;
//!
//! let auth = SecretString::from("Bearer xyz");
//! let api_url: Url = "https://api.example.com/jmap/".parse().unwrap();
//! let coroutine = JmapQueryChanges::new::<Value, Value>(
//!     "a1".into(),
//!     &auth,
//!     &api_url,
//!     "Email/queryChanges",
//!     vec!["urn:ietf:params:jmap:mail".into()],
//!     "qs1",
//!     JmapQueryChangesOptions::default(),
//! )
//! .unwrap();
//! # let _ = coroutine;
//! ```

use core::fmt;

use alloc::{string::String, vec::Vec};

use log::trace;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{JmapAddedItem, JmapBatch, JmapMethodError, send::*},
};

/// Failure causes during a JMAP `Foo/queryChanges` flow.
#[derive(Debug, Error)]
pub enum JmapQueryChangesError {
    #[error("JMAP Foo/queryChanges failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP Foo/queryChanges failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Foo/queryChanges failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Foo/queryChanges failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP Foo/queryChanges failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Options for [`JmapQueryChanges::new`].
#[derive(Clone, Debug)]
pub struct JmapQueryChangesOptions<F: Serialize, S: Serialize> {
    pub filter: Option<F>,
    pub sort: Option<Vec<S>>,
    pub max_changes: Option<u64>,
    pub up_to_id: Option<String>,
    /// Ask the server to compute `total`. Off by default.
    pub calculate_total: bool,
}

impl<F: Serialize, S: Serialize> Default for JmapQueryChangesOptions<F, S> {
    fn default() -> Self {
        Self {
            filter: None,
            sort: None,
            max_changes: None,
            up_to_id: None,
            calculate_total: false,
        }
    }
}

/// Successful terminal output of [`JmapQueryChanges`].
#[derive(Clone, Debug)]
pub struct JmapQueryChangesOutput {
    pub new_query_state: String,
    pub total: Option<u64>,
    pub removed: Vec<String>,
    pub added: Vec<JmapAddedItem>,
    pub keep_alive: bool,
}

/// Generic I/O-free coroutine for the JMAP `Foo/queryChanges` method
/// (RFC 8620 §5.6).
pub struct JmapQueryChanges {
    state: State,
}

impl JmapQueryChanges {
    /// Builds a single-call `Foo/queryChanges` batch and wraps it in
    /// [`JmapSend`].
    pub fn new<F: Serialize, S: Serialize>(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        since_query_state: impl Into<String>,
        opts: JmapQueryChangesOptions<F, S>,
    ) -> Result<Self, JmapQueryChangesError> {
        let since_query_state = since_query_state.into();
        let args = serde_json::to_value(QueryChangesArgs {
            account_id: &account_id,
            filter: opts.filter,
            sort: opts.sort,
            since_query_state: &since_query_state,
            max_changes: opts.max_changes,
            up_to_id: opts.up_to_id,
            calculate_total: opts.calculate_total,
        })
        .map_err(JmapQueryChangesError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add(method, args);
        let request = batch.into_request(capabilities);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapQueryChanges {
    type Yield = JmapYield;
    type Return = Result<JmapQueryChangesOutput, JmapQueryChangesError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("query-changes: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapQueryChangesError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<QueryChangesResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapQueryChangesOutput {
                        new_query_state: r.new_query_state,
                        total: r.total,
                        removed: r.removed,
                        added: r.added,
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapQueryChangesError::ParseResponse(err)))
                    }
                }
            }
        }
    }
}

enum State {
    Send(JmapSend),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryChangesArgs<'a, F: Serialize, S: Serialize> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<F>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Vec<S>>,
    since_query_state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_changes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    up_to_id: Option<String>,
    calculate_total: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryChangesResponse {
    new_query_state: String,
    #[serde(default)]
    total: Option<u64>,
    removed: Vec<String>,
    added: Vec<JmapAddedItem>,
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString, vec};

    use super::*;

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_url() -> Url {
        "https://api.example.com/jmap/".parse().unwrap()
    }

    fn make_query_changes() -> JmapQueryChanges {
        JmapQueryChanges::new::<serde_json::Value, serde_json::Value>(
            "a1".to_string(),
            &make_auth(),
            &make_url(),
            "Email/queryChanges",
            vec!["urn:ietf:params:jmap:mail".to_string()],
            "qs1",
            JmapQueryChangesOptions::default(),
        )
        .unwrap()
    }

    fn build_http_reply(body: &[u8]) -> Vec<u8> {
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
            body.len()
        );
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }

    #[test]
    fn success_returns_ok() {
        let mut cor = make_query_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Email/queryChanges", {"newQueryState":"qs2","removed":["e2"],"added":[{"id":"e1","index":0}]}, "c0"]],
            "sessionState": "s2"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.new_query_state, "qs2");
        assert_eq!(out.removed, vec!["e2".to_string()]);
        assert_eq!(out.added.len(), 1);
    }

    #[test]
    fn method_error_returns_method_error() {
        let mut cor = make_query_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["error", {"type":"cannotCalculateChanges"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapQueryChangesError::Method(_)));
    }

    #[test]
    fn missing_response_returns_missing_response() {
        let mut cor = make_query_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(br#"{"methodResponses":[], "sessionState":"s"}"#);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapQueryChangesError::MissingResponse));
    }

    #[test]
    fn parse_error_returns_parse_response() {
        let mut cor = make_query_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Email/queryChanges", {"newQueryState":42}, "c0"]],
            "sessionState": "s"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapQueryChangesError::ParseResponse(_)));
    }

    #[test]
    fn total_optional_when_calculate_total_set() {
        let mut cor = make_query_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Email/queryChanges", {"newQueryState":"qs2","removed":[],"added":[],"total":7}, "c0"]],
            "sessionState": "s"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.total, Some(7));
    }

    // --- utils

    fn expect_wants_write(cor: &mut JmapQueryChanges, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapQueryChanges) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut JmapQueryChanges, reply: &[u8]) -> JmapQueryChangesOutput {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(cor: &mut JmapQueryChanges, reply: &[u8]) -> JmapQueryChangesError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
