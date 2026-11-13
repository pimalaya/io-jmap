//! JMAP `ContactCard/copy` coroutine (RFC 9610 §3.6): copies cards between
//! accounts per the standard `/copy` method (RFC 8620 §5.4).
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     collections::BTreeMap,
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_jmap::{
//!     coroutine::{JmapCoroutine, JmapCoroutineState, JmapYield},
//!     rfc8620::session::JmapSession,
//!     rfc9610::contact_card::copy::JmapContactCardCopy,
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
//!     "primaryAccounts": {"urn:ietf:params:jmap:contacts": "a1"},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine =
//!     JmapContactCardCopy::new(&session, &auth, "a2", BTreeMap::new()).unwrap();
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
//! println!("{} created", out.created.len());
//! ```

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY,
        error::JmapMethodError,
        request::JmapBatch,
        send::*,
        session::{JmapSession, JmapSessionAccountError},
        set::SetResponse,
    },
    rfc9610::{JMAP_CONTACTS_CAPABILITY, contact_card::JmapContactCard},
};

/// Arguments for copying a single card between accounts via
/// `ContactCard/copy` (RFC 9610 §3.6).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapContactCardCopyArgs {
    /// Source ContactCard id.
    pub id: String,
    /// `{ address-book-id -> true }` in the destination account.
    pub address_book_ids: BTreeMap<String, bool>,
}

/// Per-object error returned in `ContactCard/copy` responses (RFC 8620
/// §5.4).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapContactCardCopyItemError {
    /// The server forbids duplicates and the card already exists in the
    /// destination account (RFC 8620 §5.4).
    AlreadyExists {
        /// The id of the existing card.
        #[serde(rename = "existingId")]
        existing_id: String,
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): one or more properties were
    /// invalid.
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

/// Failure causes during a JMAP `ContactCard/copy` flow.
#[derive(Debug, Error)]
pub enum JmapContactCardCopyError {
    /// The session names no account to address the call to.
    #[error("JMAP ContactCard/copy failed: {0}")]
    Account(#[from] JmapSessionAccountError),
    /// The response carried no method response.
    #[error("JMAP ContactCard/copy failed: missing response in method_responses")]
    MissingResponse,
    /// The inner send coroutine failed.
    #[error("JMAP ContactCard/copy failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP ContactCard/copy failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The method response could not be parsed.
    #[error("JMAP ContactCard/copy failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    /// The server returned a method-level error.
    #[error("JMAP ContactCard/copy failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapContactCardCopy`].
#[derive(Clone, Debug)]
pub struct JmapContactCardCopyOutput {
    /// The new server state after the call.
    pub new_state: String,
    /// The created cards, keyed by client id.
    pub created: BTreeMap<String, JmapContactCard>,
    /// The failed copies, keyed by client id.
    pub not_created: BTreeMap<String, JmapContactCardCopyItemError>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `ContactCard/copy` method.
pub struct JmapContactCardCopy {
    state: State,
}

impl JmapContactCardCopy {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        from_account_id: impl Into<String>,
        cards: BTreeMap<String, JmapContactCardCopyArgs>,
    ) -> Result<Self, JmapContactCardCopyError> {
        let account_id = String::from(session.account_for(JMAP_CONTACTS_CAPABILITY)?);
        let api_url = &session.api_url;

        let args = serde_json::to_value(ContactCardCopyArgs {
            from_account_id: from_account_id.into(),
            account_id,
            create: cards,
        })
        .map_err(JmapContactCardCopyError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("ContactCard/copy", args);
        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_CONTACTS_CAPABILITY.into(),
        ]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapContactCardCopy {
    type Yield = JmapYield;
    type Return = Result<JmapContactCardCopyOutput, JmapContactCardCopyError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapContactCardCopyError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or_else(JmapMethodError::unreadable);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<
                    SetResponse<JmapContactCard, JmapContactCardCopyItemError>,
                >(args)
                {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapContactCardCopyOutput {
                        new_state: r.new_state.unwrap_or_default(),
                        created: r.created.unwrap_or_default(),
                        not_created: r.not_created.unwrap_or_default(),
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapContactCardCopyError::ParseResponse(err),
                    )),
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
struct ContactCardCopyArgs {
    from_account_id: String,
    account_id: String,
    create: BTreeMap<String, JmapContactCardCopyArgs>,
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString};

    use crate::rfc9610::contact_card::copy::*;

    #[test]
    fn refusal_without_new_state_returns_ok() {
        let session: JmapSession = serde_json::from_value(serde_json::json!({
            "username": "",
            "accounts": {},
            "primaryAccounts": { "urn:ietf:params:jmap:contacts": "a1" },
            "capabilities": {},
            "apiUrl": "https://api.example.com/jmap/",
            "downloadUrl": "",
            "uploadUrl": "",
            "eventSourceUrl": "",
            "state": "",
        }))
        .unwrap();
        let copy = JmapContactCardCopyArgs {
            id: "k1".to_string(),
            address_book_ids: BTreeMap::from([("b1".to_string(), true)]),
        };
        let cards = BTreeMap::from([("c1".to_string(), copy)]);
        let auth = SecretString::from("Bearer t");
        let mut coroutine = JmapContactCardCopy::new(&session, &auth, "a0", cards).unwrap();

        let body = br#"{
            "methodResponses": [["ContactCard/copy", {
                "fromAccountId": "a0", "accountId": "a1",
                "notCreated": {"c1": {"type": "notFound"}}
            }, "c0"]],
            "sessionState": "s1"
        }"#;
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
        let out = match coroutine.resume(Some(&reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        };

        assert_eq!(out.new_state, "");
        assert!(matches!(
            out.not_created["c1"],
            JmapContactCardCopyItemError::NotFound { .. }
        ));
    }
}
