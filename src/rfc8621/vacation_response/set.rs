//! JMAP `VacationResponse/set` coroutine (RFC 8621 §8.3): updates the singleton
//! VacationResponse (id `"singleton"`).
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
//!     rfc8620::session::JmapSession,
//!     rfc8621::vacation_response::set::{JmapVacationResponseSet, JmapVacationResponseUpdate},
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
//!     "primaryAccounts": {"urn:ietf:params:jmap:mail": "a1"},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let patch = JmapVacationResponseUpdate {
//!     is_enabled: Some(true),
//!     subject: Some(Some("Out of office".into())),
//!     ..Default::default()
//! };
//! let mut coroutine = JmapVacationResponseSet::new(&session, &auth, patch).unwrap();
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
//! println!("new state {}", out.new_state);
//! ```

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY,
        error::{JmapMethodError, write_described, write_properties},
        request::JmapBatch,
        send::*,
        session::JmapSession,
    },
    rfc8621::{
        JMAP_MAIL_CAPABILITY,
        vacation_response::{JMAP_VACATION_RESPONSE_CAPABILITY, JmapVacationResponse},
    },
};

/// Patch object for `VacationResponse/set` update (RFC 8621 §8).
///
/// A `None` field is left unchanged, `Some(None)` clears it (`null`), as
/// RFC 8620 §5.3 patches a property.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapVacationResponseUpdate {
    /// Whether the vacation response is sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
    /// RFC 3339 start of the vacation period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_date: Option<Option<String>>,
    /// RFC 3339 end of the vacation period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_date: Option<Option<String>>,
    /// Subject of the auto-reply message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<Option<String>>,
    /// Plaintext body of the auto-reply message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_body: Option<Option<String>>,
    /// HTML body of the auto-reply message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_body: Option<Option<String>>,
}

/// Per-object error returned in `VacationResponse/set` responses
/// (RFC 8621 §8).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapVacationResponseSetItemError {
    /// Standard set error (RFC 8620 §5.3): patch could not be applied.
    InvalidPatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): one or more properties were invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// A set error type this library does not model, kept as the server
    /// named it.
    #[serde(untagged)]
    Other {
        /// The error type, empty when the server named none.
        #[serde(rename = "type")]
        type_name: String,
        /// Optional human-readable detail.
        description: Option<String>,
    },
}

impl fmt::Display for JmapVacationResponseSetItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPatch { description } => write_described(f, "invalidPatch", description),
            Self::InvalidProperties {
                description,
                properties,
            } => write_properties(f, description, properties),
            Self::Other {
                type_name,
                description,
            } => write_described(f, type_name, description),
        }
    }
}

/// Failure causes during a JMAP `VacationResponse/set` flow.
#[derive(Debug, Error)]
pub enum JmapVacationResponseSetError {
    /// The server refused the update, leaving the singleton as it was.
    #[error("JMAP VacationResponse/set failed: {0}")]
    NotUpdated(JmapVacationResponseSetItemError),
    /// The response carried no method response.
    #[error("JMAP VacationResponse/set failed: missing response in method_responses")]
    MissingResponse,
    /// The inner send coroutine failed.
    #[error("JMAP VacationResponse/set failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP VacationResponse/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The method response could not be parsed.
    #[error("JMAP VacationResponse/set failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    /// The server returned a method-level error.
    #[error("JMAP VacationResponse/set failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapVacationResponseSet`].
#[derive(Clone, Debug)]
pub struct JmapVacationResponseSetOutput {
    /// The new server state after the call; empty when the server named
    /// none, which Fastmail does.
    pub new_state: String,
    /// The updated singleton, when the server echoed it back.
    pub updated: Option<JmapVacationResponse>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `VacationResponse/set` method.
pub struct JmapVacationResponseSet {
    state: State,
}

impl JmapVacationResponseSet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        patch: JmapVacationResponseUpdate,
    ) -> Result<Self, JmapVacationResponseSetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_VACATION_RESPONSE_CAPABILITY)
            .or_else(|| session.primary_accounts.get(JMAP_MAIL_CAPABILITY))
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(VacationResponseSetArgs {
            account_id,
            update: BTreeMap::from([("singleton", patch)]),
        })
        .map_err(JmapVacationResponseSetError::SerializeArgs)?;

        let mut using = vec![JMAP_CORE_CAPABILITY.into(), JMAP_MAIL_CAPABILITY.into()];
        if session
            .capabilities
            .contains_key(JMAP_VACATION_RESPONSE_CAPABILITY)
        {
            using.push(JMAP_VACATION_RESPONSE_CAPABILITY.into());
        }

        let mut batch = JmapBatch::new();
        batch.add("VacationResponse/set", args);
        let request = batch.into_request(using);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapVacationResponseSet {
    type Yield = JmapYield;
    type Return = Result<JmapVacationResponseSetOutput, JmapVacationResponseSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapVacationResponseSetError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or_else(JmapMethodError::unreadable);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                let r = match serde_json::from_value::<VacationResponseSetResponse>(args) {
                    Ok(r) => r,
                    Err(err) => {
                        let err = JmapVacationResponseSetError::ParseResponse(err);
                        return JmapCoroutineState::Complete(Err(err));
                    }
                };

                if let Some(refusal) = r.not_updated.unwrap_or_default().into_values().next() {
                    let refusal = serde_json::from_value(refusal).unwrap_or(
                        JmapVacationResponseSetItemError::Other {
                            type_name: String::new(),
                            description: None,
                        },
                    );
                    let err = JmapVacationResponseSetError::NotUpdated(refusal);
                    return JmapCoroutineState::Complete(Err(err));
                }

                JmapCoroutineState::Complete(Ok(JmapVacationResponseSetOutput {
                    new_state: r.new_state.unwrap_or_default(),
                    updated: r
                        .updated
                        .unwrap_or_default()
                        .into_values()
                        .flatten()
                        .find_map(|value| serde_json::from_value(value).ok()),
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Send(JmapSend),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseSetArgs {
    account_id: String,
    update: BTreeMap<&'static str, JmapVacationResponseUpdate>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VacationResponseSetResponse {
    // NOTE: RFC 8620 §5.3 requires it, but Fastmail answers `null`.
    #[serde(default)]
    new_state: Option<String>,
    // NOTE: RFC 8620 §5.3 echoes only the properties the server changed
    // itself, which Fastmail fills with the ones it was sent: a partial
    // object, read as a whole response only when it is one.
    #[serde(default)]
    updated: Option<BTreeMap<String, Option<serde_json::Value>>>,
    not_updated: Option<BTreeMap<String, serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString};

    use super::*;

    /// A field left `None` stays out of the patch, `Some(None)` clears it.
    #[test]
    fn a_cleared_field_is_sent_as_null() {
        let patch = JmapVacationResponseUpdate {
            subject: Some(None),
            text_body: Some(Some("away".into())),
            ..Default::default()
        };

        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            serde_json::json!({ "subject": null, "textBody": "away" })
        );
    }

    /// Fastmail answers `newState: null` to an update it applied, echoing
    /// the properties it was sent as a partial object.
    #[test]
    fn a_null_new_state_reads_as_empty() {
        let out = set(br#"{
            "methodResponses": [["VacationResponse/set", {
                "accountId": "a1", "oldState": null, "newState": null,
                "updated": {"singleton": {"fromDate": null}}
            }, "c0"]],
            "sessionState": "s1"
        }"#)
        .expect("the update did not complete");

        assert!(out.new_state.is_empty());
        assert!(out.updated.is_none());
    }

    #[test]
    fn a_refused_update_returns_an_error() {
        let err = set(br#"{
            "methodResponses": [["VacationResponse/set", {
                "accountId": "a1", "oldState": "v1", "newState": "v1",
                "updated": null,
                "notUpdated": {"singleton": {
                    "type": "invalidProperties", "properties": ["toDate"]
                }}
            }, "c0"]],
            "sessionState": "s1"
        }"#)
        .expect_err("the refusal read as a success");

        assert!(matches!(
            &err,
            JmapVacationResponseSetError::NotUpdated(
                JmapVacationResponseSetItemError::InvalidProperties { properties, .. }
            ) if properties == &["toDate"]
        ));
        assert_eq!(
            err.to_string(),
            "JMAP VacationResponse/set failed: JMAP invalidProperties [toDate]"
        );
    }

    /// Runs one `VacationResponse/set` against the response `body`.
    fn set(body: &[u8]) -> Result<JmapVacationResponseSetOutput, JmapVacationResponseSetError> {
        let session: JmapSession = serde_json::from_value(serde_json::json!({
            "username": "",
            "accounts": {},
            "primaryAccounts": { "urn:ietf:params:jmap:mail": "a1" },
            "capabilities": {},
            "apiUrl": "https://api.example.com/jmap/",
            "downloadUrl": "",
            "uploadUrl": "",
            "eventSourceUrl": "",
            "state": "",
        }))
        .unwrap();
        let patch = JmapVacationResponseUpdate {
            is_enabled: Some(false),
            ..Default::default()
        };
        let mut coroutine =
            JmapVacationResponseSet::new(&session, &SecretString::from("Bearer t"), patch).unwrap();

        let mut reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
            body.len()
        )
        .into_bytes();
        reply.extend_from_slice(body);

        assert!(matches!(
            coroutine.resume(None),
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(_))
        ));
        assert!(matches!(
            coroutine.resume(None),
            JmapCoroutineState::Yielded(JmapYield::WantsRead)
        ));
        match coroutine.resume(Some(&reply)) {
            JmapCoroutineState::Complete(out) => out,
            state => panic!("expected Complete, got {state:?}"),
        }
    }
}
