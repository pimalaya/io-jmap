//! JMAP `PushSubscription/get` coroutine (RFC 8620 §7.2.1): builds a custom
//! get batch (no generic [`JmapGet`](crate::rfc8620::get::JmapGet) reuse
//! because the method takes no `accountId` and returns no `state`).
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_jmap::{
//!     coroutine::{JmapCoroutine, JmapCoroutineState, JmapYield},
//!     rfc8620::{
//!         JmapSession,
//!         push_subscription::get::{JmapPushSubscriptionGet, JmapPushSubscriptionGetOptions},
//!     },
//! };
//! use secrecy::SecretString;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let session: JmapSession = serde_json::from_str(r#"{
//!     "username": "",
//!     "accounts": {},
//!     "primaryAccounts": {},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine = JmapPushSubscriptionGet::new(
//!     &session,
//!     &auth,
//!     JmapPushSubscriptionGetOptions::default(),
//! )
//! .unwrap();
//! let mut arg = None;
//!
//! let out = loop {
//!     match coroutine.resume(arg.take()) {
//!         JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         JmapCoroutineState::Yielded(JmapYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         JmapCoroutineState::Complete(Ok(out)) => break out,
//!         JmapCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} push subscriptions", out.subscriptions.len());
//! ```

use core::fmt;

use alloc::{string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{
        CORE_CAPABILITY, JmapBatch, JmapMethodError, JmapSession,
        push_subscription::JmapPushSubscription, send::*,
    },
};

/// Failure causes during a JMAP `PushSubscription/get` flow.
#[derive(Debug, Error)]
pub enum JmapPushSubscriptionGetError {
    #[error("JMAP PushSubscription/get failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP PushSubscription/get failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP PushSubscription/get failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP PushSubscription/get failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP PushSubscription/get failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Options for [`JmapPushSubscriptionGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapPushSubscriptionGetOptions {
    /// Restrict the fetch to these subscription IDs; `None` fetches all.
    pub ids: Option<Vec<String>>,
    /// Restrict the returned properties; `None` returns all but `url` and
    /// `keys`. Requesting `url` or `keys` is rejected with a `forbidden`
    /// error (RFC 8620 §7.2.1).
    pub properties: Option<Vec<String>>,
}

/// Successful terminal output of [`JmapPushSubscriptionGet`].
#[derive(Clone, Debug)]
pub struct JmapPushSubscriptionGetOutput {
    pub subscriptions: Vec<JmapPushSubscription>,
    pub not_found: Vec<String>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `PushSubscription/get` method.
pub struct JmapPushSubscriptionGet {
    state: State,
}

impl JmapPushSubscriptionGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapPushSubscriptionGetOptions,
    ) -> Result<Self, JmapPushSubscriptionGetError> {
        let args = serde_json::to_value(PushSubscriptionGetArgs {
            ids: opts.ids.as_deref(),
            properties: opts.properties.as_deref(),
        })
        .map_err(JmapPushSubscriptionGetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("PushSubscription/get", args);
        let request = batch.into_request(vec![CORE_CAPABILITY.into()]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, &session.api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapPushSubscriptionGet {
    type Yield = JmapYield;
    type Return = Result<JmapPushSubscriptionGetOutput, JmapPushSubscriptionGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("PushSubscription/get: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapPushSubscriptionGetError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<PushSubscriptionGetResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapPushSubscriptionGetOutput {
                        subscriptions: r.list,
                        not_found: r.not_found,
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapPushSubscriptionGetError::ParseResponse(err),
                    )),
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
struct PushSubscriptionGetArgs<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    ids: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [String]>,
}

/// No `state` field: `PushSubscription/get` does not return one (RFC 8620
/// §7.2.1).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushSubscriptionGetResponse {
    list: Vec<JmapPushSubscription>,
    #[serde(default)]
    not_found: Vec<String>,
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString};

    use super::*;

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_session() -> JmapSession {
        serde_json::from_str(
            r#"{
                "username": "",
                "accounts": {},
                "primaryAccounts": {},
                "capabilities": {},
                "apiUrl": "https://api.example.com/jmap/",
                "downloadUrl": "",
                "uploadUrl": "",
                "eventSourceUrl": "",
                "state": ""
            }"#,
        )
        .unwrap()
    }

    fn make_get() -> JmapPushSubscriptionGet {
        JmapPushSubscriptionGet::new(
            &make_session(),
            &make_auth(),
            JmapPushSubscriptionGetOptions::default(),
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
    fn request_omits_account_id() {
        let mut cor = make_get();
        let bytes = expect_wants_write(&mut cor, None);
        let request = String::from_utf8(bytes).unwrap();
        assert!(!request.contains("accountId"));
    }

    #[test]
    fn success_returns_ok_without_state() {
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["PushSubscription/get", {
                "list": [{
                    "id": "P1",
                    "deviceClientId": "a889-ffea-910",
                    "verificationCode": "b210ef734fe5f439c1ca386421359f7b",
                    "expires": "2018-07-31T00:13:21Z",
                    "types": ["Email"]
                }],
                "notFound": []
            }, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.subscriptions.len(), 1);
        assert_eq!(out.subscriptions[0].id, "P1");
        assert_eq!(
            out.subscriptions[0].device_client_id.as_deref(),
            Some("a889-ffea-910")
        );
        assert!(out.not_found.is_empty());
    }

    #[test]
    fn method_error_returns_method_error() {
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["error", {"type":"forbidden"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapPushSubscriptionGetError::Method(_)));
    }

    #[test]
    fn missing_response_returns_missing_response() {
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(br#"{"methodResponses":[], "sessionState":"s1"}"#);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapPushSubscriptionGetError::MissingResponse));
    }

    #[test]
    fn parse_error_returns_parse_response() {
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["PushSubscription/get", {"list":"nope"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(
            err,
            JmapPushSubscriptionGetError::ParseResponse(_)
        ));
    }

    #[test]
    fn http_error_surfaces_as_send_error() {
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
        let err = expect_complete_err(&mut cor, reply);
        assert!(matches!(
            err,
            JmapPushSubscriptionGetError::Send(JmapSendError::HttpStatus(401))
        ));
    }

    // --- utils

    fn expect_wants_write(cor: &mut JmapPushSubscriptionGet, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapPushSubscriptionGet) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(
        cor: &mut JmapPushSubscriptionGet,
        reply: &[u8],
    ) -> JmapPushSubscriptionGetOutput {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut JmapPushSubscriptionGet,
        reply: &[u8],
    ) -> JmapPushSubscriptionGetError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
