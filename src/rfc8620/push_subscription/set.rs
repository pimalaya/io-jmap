//! JMAP `PushSubscription/set` coroutine (RFC 8620 §7.2.2): builds a custom
//! set batch (no generic [`JmapSet`](crate::rfc8620::set::JmapSet) reuse
//! because the method takes no `accountId` or `ifInState` and returns no
//! `oldState`/`newState`).
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
//!         push_subscription::{
//!             JmapPushSubscriptionCreate,
//!             set::{JmapPushSubscriptionSet, JmapPushSubscriptionSetArgs},
//!         },
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
//! let mut args = JmapPushSubscriptionSetArgs::default();
//! args.create(
//!     "c1",
//!     JmapPushSubscriptionCreate {
//!         device_client_id: "a889-ffea-910".into(),
//!         url: "https://push.example.com/?device=X8980fc".into(),
//!         ..Default::default()
//!     },
//! );
//! let mut coroutine = JmapPushSubscriptionSet::new(&session, &auth, args).unwrap();
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
//! println!("created {} push subscriptions", out.created.len());
//! ```

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{
        CORE_CAPABILITY, JmapBatch, JmapMethodError, JmapSession, JmapSetError,
        push_subscription::{
            JmapPushSubscription, JmapPushSubscriptionCreate, JmapPushSubscriptionUpdate,
        },
        send::*,
    },
};

/// Failure causes during a JMAP `PushSubscription/set` flow.
#[derive(Debug, Error)]
pub enum JmapPushSubscriptionSetError {
    #[error("JMAP PushSubscription/set failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP PushSubscription/set failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP PushSubscription/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP PushSubscription/set failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP PushSubscription/set failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Arguments for a `PushSubscription/set` request.
#[derive(Clone, Debug, Default)]
pub struct JmapPushSubscriptionSetArgs {
    pub create: BTreeMap<String, JmapPushSubscriptionCreate>,
    pub update: BTreeMap<String, JmapPushSubscriptionUpdate>,
    pub destroy: Vec<String>,
}

impl JmapPushSubscriptionSetArgs {
    pub fn create(
        &mut self,
        client_id: impl Into<String>,
        subscription: JmapPushSubscriptionCreate,
    ) -> &mut Self {
        self.create.insert(client_id.into(), subscription);
        self
    }

    pub fn update(
        &mut self,
        id: impl Into<String>,
        patch: JmapPushSubscriptionUpdate,
    ) -> &mut Self {
        self.update.insert(id.into(), patch);
        self
    }

    pub fn destroy(&mut self, id: impl Into<String>) -> &mut Self {
        self.destroy.push(id.into());
        self
    }
}

/// Successful terminal output of [`JmapPushSubscriptionSet`].
///
/// No state strings: `PushSubscription/set` returns no `oldState`/`newState`
/// (RFC 8620 §7.2.2).
#[derive(Clone, Debug)]
pub struct JmapPushSubscriptionSetOutput {
    pub created: BTreeMap<String, JmapPushSubscription>,
    pub updated: BTreeMap<String, Option<JmapPushSubscription>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, JmapSetError>,
    pub not_updated: BTreeMap<String, JmapSetError>,
    pub not_destroyed: BTreeMap<String, JmapSetError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `PushSubscription/set` method.
pub struct JmapPushSubscriptionSet {
    state: State,
}

impl JmapPushSubscriptionSet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapPushSubscriptionSetArgs,
    ) -> Result<Self, JmapPushSubscriptionSetError> {
        let json_args = serde_json::to_value(PushSubscriptionSetRequest {
            create: args.create,
            update: args.update,
            destroy: args.destroy,
        })
        .map_err(JmapPushSubscriptionSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("PushSubscription/set", json_args);
        let request = batch.into_request(vec![CORE_CAPABILITY.into()]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, &session.api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapPushSubscriptionSet {
    type Yield = JmapYield;
    type Return = Result<JmapPushSubscriptionSetOutput, JmapPushSubscriptionSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("PushSubscription/set: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapPushSubscriptionSetError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<PushSubscriptionSetResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapPushSubscriptionSetOutput {
                        created: r.created.unwrap_or_default(),
                        updated: r.updated.unwrap_or_default(),
                        destroyed: r.destroyed.unwrap_or_default(),
                        not_created: r.not_created.unwrap_or_default(),
                        not_updated: r.not_updated.unwrap_or_default(),
                        not_destroyed: r.not_destroyed.unwrap_or_default(),
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapPushSubscriptionSetError::ParseResponse(err),
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
struct PushSubscriptionSetRequest {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    create: BTreeMap<String, JmapPushSubscriptionCreate>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    update: BTreeMap<String, JmapPushSubscriptionUpdate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    destroy: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushSubscriptionSetResponse {
    #[serde(default)]
    created: Option<BTreeMap<String, JmapPushSubscription>>,
    #[serde(default)]
    updated: Option<BTreeMap<String, Option<JmapPushSubscription>>>,
    #[serde(default)]
    destroyed: Option<Vec<String>>,
    #[serde(default)]
    not_created: Option<BTreeMap<String, JmapSetError>>,
    #[serde(default)]
    not_updated: Option<BTreeMap<String, JmapSetError>>,
    #[serde(default)]
    not_destroyed: Option<BTreeMap<String, JmapSetError>>,
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

    fn make_set() -> JmapPushSubscriptionSet {
        let mut args = JmapPushSubscriptionSetArgs::default();
        args.create(
            "c1",
            JmapPushSubscriptionCreate {
                device_client_id: "a889-ffea-910".to_string(),
                url: "https://push.example.com/?device=X8980fc".to_string(),
                ..Default::default()
            },
        );
        JmapPushSubscriptionSet::new(&make_session(), &make_auth(), args).unwrap()
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
    fn request_omits_account_id_and_if_in_state() {
        let mut cor = make_set();
        let bytes = expect_wants_write(&mut cor, None);
        let request = String::from_utf8(bytes).unwrap();
        assert!(!request.contains("accountId"));
        assert!(!request.contains("ifInState"));
    }

    #[test]
    fn success_returns_ok_without_state() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["PushSubscription/set", {
                "created": {
                    "c1": {
                        "id": "P1",
                        "keys": null,
                        "expires": "2018-07-13T02:14:29Z"
                    }
                }
            }, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.created["c1"].id, "P1");
        assert_eq!(
            out.created["c1"].expires.as_deref(),
            Some("2018-07-13T02:14:29Z")
        );
    }

    #[test]
    fn updated_echo_without_id_parses() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["PushSubscription/set", {
                "updated": {
                    "P1": { "expires": "2018-07-15T02:22:50Z" }
                }
            }, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        let echo = out.updated["P1"].as_ref().unwrap();
        assert!(echo.id.is_empty());
        assert_eq!(echo.expires.as_deref(), Some("2018-07-15T02:22:50Z"));
    }

    #[test]
    fn invalid_verification_code_surfaces_in_not_updated() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["PushSubscription/set", {
                "notUpdated": {
                    "P1": {
                        "type": "invalidProperties",
                        "properties": ["verificationCode"]
                    }
                }
            }, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.not_updated["P1"].r#type, "invalidProperties");
    }

    #[test]
    fn method_error_returns_method_error() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["error", {"type":"invalidArguments"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapPushSubscriptionSetError::Method(_)));
    }

    #[test]
    fn missing_response_returns_missing_response() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(br#"{"methodResponses":[], "sessionState":"s1"}"#);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapPushSubscriptionSetError::MissingResponse));
    }

    #[test]
    fn parse_error_returns_parse_response() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["PushSubscription/set", {"created":42}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(
            err,
            JmapPushSubscriptionSetError::ParseResponse(_)
        ));
    }

    #[test]
    fn http_error_surfaces_as_send_error() {
        let mut cor = make_set();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
        let err = expect_complete_err(&mut cor, reply);
        assert!(matches!(
            err,
            JmapPushSubscriptionSetError::Send(JmapSendError::HttpStatus(401))
        ));
    }

    // --- utils

    fn expect_wants_write(cor: &mut JmapPushSubscriptionSet, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapPushSubscriptionSet) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(
        cor: &mut JmapPushSubscriptionSet,
        reply: &[u8],
    ) -> JmapPushSubscriptionSetOutput {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut JmapPushSubscriptionSet,
        reply: &[u8],
    ) -> JmapPushSubscriptionSetError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
