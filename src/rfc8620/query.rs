//! Generic JMAP `Foo/query` coroutine (RFC 8620 §5.5): wraps [`JmapSend`] with
//! a single filter+sort batch and decodes the id list.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::rfc8620::query::{JmapQuery, JmapQueryOptions};
//! use secrecy::SecretString;
//! use serde_json::Value;
//! use url::Url;
//!
//! let auth = SecretString::from("Bearer xyz");
//! let api_url: Url = "https://api.example.com/jmap/".parse().unwrap();
//! let coroutine = JmapQuery::new::<Value, Value>(
//!     "a1".into(),
//!     &auth,
//!     &api_url,
//!     "Email/query",
//!     vec!["urn:ietf:params:jmap:mail".into()],
//!     JmapQueryOptions { limit: Some(10), ..Default::default() },
//! )
//! .unwrap();
//! # let _ = coroutine;
//! ```

use alloc::{string::String, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{JmapBatch, JmapMethodError, send::*},
};

/// Failure causes during a JMAP `Foo/query` flow.
#[derive(Debug, Error)]
pub enum JmapQueryError {
    /// The response carried no method response.
    #[error("JMAP Foo/query failed: missing response in method_responses")]
    MissingResponse,
    /// The inner send coroutine failed.
    #[error("JMAP Foo/query failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP Foo/query failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The method response could not be parsed.
    #[error("JMAP Foo/query failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    /// The server returned a method-level error.
    #[error("JMAP Foo/query failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Options for [`JmapQuery::new`].
#[derive(Clone, Debug)]
pub struct JmapQueryOptions<F: Serialize, S: Serialize> {
    /// The filter conditions the objects must match.
    pub filter: Option<F>,
    /// The sort comparators applied to the results.
    pub sort: Option<Vec<S>>,
    /// Zero-based index of the first result to return.
    pub position: Option<u64>,
    /// Id of the object to anchor the result window on.
    pub anchor: Option<String>,
    /// Offset of the result window relative to the anchor.
    pub anchor_offset: Option<i64>,
    /// Maximum number of results to return.
    pub limit: Option<u64>,
    /// Ask the server to compute `total`. Off by default.
    pub calculate_total: bool,
}

impl<F: Serialize, S: Serialize> Default for JmapQueryOptions<F, S> {
    fn default() -> Self {
        Self {
            filter: None,
            sort: None,
            position: None,
            anchor: None,
            anchor_offset: None,
            limit: None,
            calculate_total: false,
        }
    }
}

/// Successful terminal output of [`JmapQuery`].
#[derive(Clone, Debug)]
pub struct JmapQueryOutput {
    /// The state the query results were computed at.
    pub query_state: String,
    /// Whether the server can compute query changes from this state.
    pub can_calculate_changes: bool,
    /// Zero-based index of the first returned id.
    pub position: u64,
    /// The matching object ids in sorted order.
    pub ids: Vec<String>,
    /// The total number of matching objects, when the server computed it.
    pub total: Option<u64>,
    /// Maximum number of results to return.
    pub limit: Option<u64>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// Generic I/O-free coroutine for the JMAP `Foo/query` method (RFC 8620 §5.5).
pub struct JmapQuery {
    state: State,
}

impl JmapQuery {
    /// Builds a single-call `Foo/query` batch and wraps it in [`JmapSend`].
    pub fn new<F: Serialize, S: Serialize>(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        opts: JmapQueryOptions<F, S>,
    ) -> Result<Self, JmapQueryError> {
        let args = serde_json::to_value(QueryArgs {
            account_id: &account_id,
            filter: opts.filter,
            sort: opts.sort,
            position: opts.position,
            anchor: opts.anchor,
            anchor_offset: opts.anchor_offset,
            limit: opts.limit,
            calculate_total: opts.calculate_total,
        })
        .map_err(JmapQueryError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add(method, args);
        let request = batch.into_request(capabilities);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }

    /// Wraps a pre-built [`JmapSend`].
    pub fn from_send(send: JmapSend) -> Self {
        Self {
            state: State::Send(send),
        }
    }
}

impl JmapCoroutine for JmapQuery {
    type Yield = JmapYield;
    type Return = Result<JmapQueryOutput, JmapQueryError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(JmapQueryError::MissingResponse));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<QueryResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapQueryOutput {
                        query_state: r.query_state,
                        can_calculate_changes: r.can_calculate_changes,
                        position: r.position,
                        ids: r.ids,
                        total: r.total,
                        limit: r.limit,
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapQueryError::ParseResponse(err)))
                    }
                }
            }
        }
    }
}

enum State {
    Send(JmapSend),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryArgs<'a, F: Serialize, S: Serialize> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<F>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Vec<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor_offset: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    calculate_total: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryResponse {
    query_state: String,
    #[serde(default)]
    can_calculate_changes: bool,
    #[serde(default)]
    position: u64,
    ids: Vec<String>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    limit: Option<u64>,
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString, vec};

    use crate::rfc8620::query::*;

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_url() -> Url {
        "https://api.example.com/jmap/".parse().unwrap()
    }

    fn make_query() -> JmapQuery {
        JmapQuery::new::<serde_json::Value, serde_json::Value>(
            "a1".to_string(),
            &make_auth(),
            &make_url(),
            "Email/query",
            vec!["urn:ietf:params:jmap:mail".to_string()],
            JmapQueryOptions {
                limit: Some(10),
                ..Default::default()
            },
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
        let mut cor = make_query();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Email/query", {"queryState":"qs","position":0,"ids":["e1","e2"]}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.ids, vec!["e1".to_string(), "e2".to_string()]);
    }

    #[test]
    fn method_error_returns_method_error() {
        let mut cor = make_query();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["error", {"type":"invalidArguments"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapQueryError::Method(_)));
    }

    #[test]
    fn missing_response_returns_missing_response() {
        let mut cor = make_query();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(br#"{"methodResponses":[], "sessionState":"s"}"#);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapQueryError::MissingResponse));
    }

    #[test]
    fn parse_error_returns_parse_response() {
        let mut cor = make_query();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Email/query", {"queryState":42}, "c0"]],
            "sessionState": "s"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapQueryError::ParseResponse(_)));
    }

    #[test]
    fn total_when_calculate_total_set() {
        let mut cor = make_query();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Email/query", {"queryState":"qs","position":0,"ids":[],"total":42}, "c0"]],
            "sessionState": "s"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.total, Some(42));
    }

    fn expect_wants_write(cor: &mut JmapQuery, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapQuery) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut JmapQuery, reply: &[u8]) -> JmapQueryOutput {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(cor: &mut JmapQuery, reply: &[u8]) -> JmapQueryError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
