//! Generic JMAP `Foo/changes` coroutine (RFC 8620 §5.2): wraps [`JmapSend`]
//! with a `since_state` cursor and decodes the changed-id lists.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::rfc8620::changes::{JmapChanges, JmapChangesOptions};
//! use secrecy::SecretString;
//! use url::Url;
//!
//! let auth = SecretString::from("Bearer xyz");
//! let api_url: Url = "https://api.example.com/jmap/".parse().unwrap();
//! let coroutine = JmapChanges::new(
//!     "a1".into(),
//!     &auth,
//!     &api_url,
//!     "Email/changes",
//!     vec!["urn:ietf:params:jmap:mail".into()],
//!     "s1",
//!     JmapChangesOptions::default(),
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
    rfc8620::{error::JmapMethodError, request::JmapBatch, send::*},
};

/// Failure causes during a JMAP `Foo/changes` flow.
#[derive(Debug, Error)]
pub enum JmapChangesError {
    /// The response carried no method response.
    #[error("JMAP Foo/changes failed: missing response in method_responses")]
    MissingResponse,
    /// The inner send coroutine failed.
    #[error("JMAP Foo/changes failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP Foo/changes failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The method response could not be parsed.
    #[error("JMAP Foo/changes failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    /// The server returned a method-level error.
    #[error("JMAP Foo/changes failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Options for [`JmapChanges::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapChangesOptions {
    /// Server-side cap on the number of changes returned. `None` lets the
    /// server pick.
    pub max_changes: Option<u64>,
}

/// Successful terminal output of [`JmapChanges`] and of the per-type wrappers
/// ([`JmapMailboxChanges`], [`JmapEmailChanges`], [`JmapThreadChanges`]).
///
/// [`JmapMailboxChanges`]: crate::rfc8621::mailbox::changes::JmapMailboxChanges
/// [`JmapEmailChanges`]: crate::rfc8621::email::changes::JmapEmailChanges
/// [`JmapThreadChanges`]: crate::rfc8621::thread::changes::JmapThreadChanges
#[derive(Clone, Debug)]
pub struct JmapChangesOutput {
    /// The new server state after the call.
    pub new_state: String,
    /// Whether more changes are available via a follow-up call.
    pub has_more_changes: bool,
    /// Ids of objects created since the requested state.
    pub created: Vec<String>,
    /// Ids of objects updated since the requested state.
    pub updated: Vec<String>,
    /// Ids of objects destroyed since the requested state.
    pub destroyed: Vec<String>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// Generic I/O-free coroutine for the JMAP `Foo/changes` method (RFC 8620
/// §5.2).
pub struct JmapChanges {
    state: State,
}

impl JmapChanges {
    /// Builds a single-call `Foo/changes` batch and wraps it in [`JmapSend`].
    pub fn new(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        since_state: impl Into<String>,
        opts: JmapChangesOptions,
    ) -> Result<Self, JmapChangesError> {
        let since_state = since_state.into();
        let args = serde_json::to_value(ChangesArgs {
            account_id: &account_id,
            since_state: &since_state,
            max_changes: opts.max_changes,
        })
        .map_err(JmapChangesError::SerializeArgs)?;

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

impl JmapCoroutine for JmapChanges {
    type Yield = JmapYield;
    type Return = Result<JmapChangesOutput, JmapChangesError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(JmapChangesError::MissingResponse));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<ChangesResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapChangesOutput {
                        new_state: r.new_state,
                        has_more_changes: r.has_more_changes,
                        created: r.created,
                        updated: r.updated,
                        destroyed: r.destroyed,
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapChangesError::ParseResponse(err)))
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
struct ChangesArgs<'a> {
    account_id: &'a str,
    since_state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_changes: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangesResponse {
    new_state: String,
    has_more_changes: bool,
    created: Vec<String>,
    updated: Vec<String>,
    destroyed: Vec<String>,
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString, vec};

    use crate::rfc8620::changes::*;

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_url() -> Url {
        "https://api.example.com/jmap/".parse().unwrap()
    }

    fn make_changes() -> JmapChanges {
        JmapChanges::new(
            "a1".to_string(),
            &make_auth(),
            &make_url(),
            "Email/changes",
            vec!["urn:ietf:params:jmap:mail".to_string()],
            "s1",
            JmapChangesOptions::default(),
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
        let mut cor = make_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Email/changes", {"newState":"s2","hasMoreChanges":false,"created":["e1"],"updated":[],"destroyed":["e2"]}, "c0"]],
            "sessionState": "s2"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.new_state, "s2");
        assert_eq!(out.created, vec!["e1".to_string()]);
        assert_eq!(out.destroyed, vec!["e2".to_string()]);
    }

    #[test]
    fn method_error_returns_method_error() {
        let mut cor = make_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["error", {"type":"cannotCalculateChanges"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapChangesError::Method(_)));
    }

    #[test]
    fn missing_response_returns_missing_response() {
        let mut cor = make_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(br#"{"methodResponses":[], "sessionState":"s"}"#);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapChangesError::MissingResponse));
    }

    #[test]
    fn parse_error_returns_parse_response() {
        let mut cor = make_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Email/changes", {"newState":42}, "c0"]],
            "sessionState": "s"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapChangesError::ParseResponse(_)));
    }

    #[test]
    fn has_more_changes_flag_propagates() {
        let mut cor = make_changes();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Email/changes", {"newState":"s2","hasMoreChanges":true,"created":[],"updated":[],"destroyed":[]}, "c0"]],
            "sessionState": "s2"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert!(out.has_more_changes);
    }

    fn expect_wants_write(cor: &mut JmapChanges, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapChanges) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut JmapChanges, reply: &[u8]) -> JmapChangesOutput {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(cor: &mut JmapChanges, reply: &[u8]) -> JmapChangesError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
