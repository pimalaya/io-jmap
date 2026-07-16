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
        JMAP_CORE_CAPABILITY, error::JmapMethodError, request::JmapBatch, send::*,
        session::JmapSession,
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
    /// The card already exists in the destination account (RFC 8620 §5.4).
    AlreadyExists {
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
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}

/// Failure causes during a JMAP `ContactCard/copy` flow.
#[derive(Debug, Error)]
pub enum JmapContactCardCopyError {
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
        let account_id = session
            .primary_accounts
            .get(JMAP_CONTACTS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
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
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<ContactCardCopyResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapContactCardCopyOutput {
                        new_state: r.new_state,
                        created: r.created,
                        not_created: r.not_created,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContactCardCopyResponse {
    new_state: String,
    #[serde(default)]
    created: BTreeMap<String, JmapContactCard>,
    #[serde(default)]
    not_created: BTreeMap<String, JmapContactCardCopyItemError>,
}
