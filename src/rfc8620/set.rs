//! Generic JMAP `Foo/set` coroutine (RFC 8620 §5.3): wraps [`JmapSend`]
//! with a single create/update/destroy batch and a typed response decoder.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::collections::BTreeMap;
//! use io_jmap::rfc8620::set::JmapSet;
//! use secrecy::SecretString;
//! use serde::{Deserialize, Serialize};
//! use url::Url;
//!
//! #[derive(Serialize)]
//! struct MailboxCreate { name: String }
//! #[derive(Deserialize, Serialize)]
//! struct Mailbox { id: String }
//!
//! let auth = SecretString::from("Bearer xyz");
//! let api_url: Url = "https://api.example.com/jmap/".parse().unwrap();
//! let mut create = BTreeMap::new();
//! create.insert("c1".to_string(), MailboxCreate { name: "Inbox".into() });
//!
//! let coroutine = JmapSet::<Mailbox>::new::<_, Mailbox>(
//!     "a1".into(),
//!     &auth,
//!     &api_url,
//!     "Mailbox/set",
//!     vec!["urn:ietf:params:jmap:mail".into()],
//!     None,
//!     Some(create),
//!     None,
//!     None,
//! )
//! .unwrap();
//! # let _ = coroutine;
//! ```

use core::{fmt, marker::PhantomData};

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use log::trace;
use secrecy::SecretString;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use url::Url;

use crate::coroutine::*;
use crate::jmap_try;
use crate::rfc8620::{JmapBatch, JmapMethodError, send::*};

/// Failure causes during a JMAP `Foo/set` flow.
#[derive(Debug, Error)]
pub enum JmapSetError {
    #[error("JMAP Foo/set failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP Foo/set failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Foo/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Foo/set failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP Foo/set failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of the [`JmapSet`] coroutine.
#[derive(Clone, Debug)]
pub struct JmapSetOutput<T> {
    pub new_state: String,
    pub created: BTreeMap<String, T>,
    pub updated: BTreeMap<String, Option<T>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, serde_json::Value>,
    pub not_updated: BTreeMap<String, serde_json::Value>,
    pub not_destroyed: BTreeMap<String, serde_json::Value>,
    pub keep_alive: bool,
}

/// Generic I/O-free coroutine for the JMAP `Foo/set` method (RFC 8620 §5.3).
pub struct JmapSet<T> {
    state: State,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> JmapSet<T> {
    /// Builds a single-call `Foo/set` batch and wraps it in [`JmapSend`].
    pub fn new<C: Serialize, U: Serialize>(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        if_in_state: Option<String>,
        create: Option<BTreeMap<String, C>>,
        update: Option<BTreeMap<String, U>>,
        destroy: Option<Vec<String>>,
    ) -> Result<Self, JmapSetError> {
        let args = serde_json::to_value(SetArgs {
            account_id,
            if_in_state,
            create,
            update,
            destroy,
        })
        .map_err(JmapSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add(method, args);

        let request = batch.into_request(capabilities);
        let send = JmapSend::new(http_auth, api_url, request)?;

        Ok(Self {
            state: State::Send(send),
            _phantom: PhantomData,
        })
    }

    /// Wraps a pre-built [`JmapSend`].
    pub fn from_send(send: JmapSend) -> Self {
        Self {
            state: State::Send(send),
            _phantom: PhantomData,
        }
    }
}

impl<T: DeserializeOwned> JmapCoroutine for JmapSet<T> {
    type Yield = JmapYield;
    type Return = Result<JmapSetOutput<T>, JmapSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("set: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(JmapSetError::MissingResponse));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<SetResponse<T>>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapSetOutput {
                        new_state: r.new_state,
                        created: r.created.unwrap_or_default(),
                        updated: r.updated.unwrap_or_default(),
                        destroyed: r.destroyed.unwrap_or_default(),
                        not_created: r.not_created.unwrap_or_default(),
                        not_updated: r.not_updated.unwrap_or_default(),
                        not_destroyed: r.not_destroyed.unwrap_or_default(),
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(JmapSetError::ParseResponse(err))),
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
struct SetArgs<C: Serialize, U: Serialize> {
    account_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    if_in_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    create: Option<BTreeMap<String, C>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<BTreeMap<String, U>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destroy: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetResponse<T> {
    new_state: String,
    created: Option<BTreeMap<String, T>>,
    updated: Option<BTreeMap<String, Option<T>>>,
    destroyed: Option<Vec<String>>,
    not_created: Option<BTreeMap<String, serde_json::Value>>,
    not_updated: Option<BTreeMap<String, serde_json::Value>>,
    not_destroyed: Option<BTreeMap<String, serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString, vec};

    use super::*;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct Probe {
        id: String,
    }

    #[derive(Serialize)]
    struct Create {
        name: String,
    }

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_url() -> Url {
        "https://api.example.com/jmap/".parse().unwrap()
    }

    fn make_set() -> JmapSet<Probe> {
        let mut create = BTreeMap::new();
        create.insert(
            "c1".to_string(),
            Create {
                name: "Inbox".into(),
            },
        );
        JmapSet::<Probe>::new::<_, Probe>(
            "a1".to_string(),
            &make_auth(),
            &make_url(),
            "Mailbox/set",
            vec!["urn:ietf:params:jmap:mail".to_string()],
            None,
            Some(create),
            None,
            None,
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
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Mailbox/set", {"newState":"s2","created":{"c1":{"id":"m1"}}}, "c0"]],
            "sessionState": "s2"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.new_state, "s2");
        assert_eq!(out.created["c1"], Probe { id: "m1".into() });
    }

    #[test]
    fn method_error_returns_method_error() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["error", {"type":"stateMismatch"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapSetError::Method(_)));
    }

    #[test]
    fn missing_response_returns_missing_response() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(br#"{"methodResponses":[], "sessionState":"s1"}"#);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapSetError::MissingResponse));
    }

    #[test]
    fn parse_error_returns_parse_response() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Mailbox/set", {"newState":42}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapSetError::ParseResponse(_)));
    }

    #[test]
    fn not_created_passthrough_succeeds() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Mailbox/set", {"newState":"s2","notCreated":{"c1":{"type":"invalidArguments"}}}, "c0"]],
            "sessionState": "s2"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert!(out.not_created.contains_key("c1"));
    }

    // --- utils

    fn expect_wants_write(cor: &mut JmapSet<Probe>, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapSet<Probe>) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut JmapSet<Probe>, reply: &[u8]) -> JmapSetOutput<Probe> {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(cor: &mut JmapSet<Probe>, reply: &[u8]) -> JmapSetError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
