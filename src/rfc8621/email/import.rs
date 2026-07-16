//! JMAP `Email/import` coroutine (RFC 8621 §4.9): imports RFC 5322 messages
//! (previously uploaded as blobs) into mailboxes. JMAP equivalent of IMAP
//! `APPEND`.
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
//!     rfc8621::email::import::{JmapEmailImport, JmapEmailImportArgs},
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
//! let mut emails = BTreeMap::new();
//! emails.insert(
//!     "c1".to_string(),
//!     JmapEmailImportArgs {
//!         blob_id: "b1".into(),
//!         mailbox_ids: Default::default(),
//!         keywords: None,
//!         received_at: None,
//!     },
//! );
//! let mut coroutine = JmapEmailImport::new(&session, &auth, emails).unwrap();
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
    rfc8621::{JMAP_MAIL_CAPABILITY, email::JmapEmail},
};

/// Arguments for importing a single RFC 5322 message via `Email/import`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailImportArgs {
    /// Blob ID of the RFC 5322 message.
    pub blob_id: String,
    /// `{ mailbox-id -> true }` for destination mailboxes.
    pub mailbox_ids: BTreeMap<String, bool>,
    /// `{ keyword -> true }` to set on the imported email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<BTreeMap<String, bool>>,
    /// RFC 3339 override for `receivedAt`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_at: Option<String>,
}

/// Per-object error returned in `Email/import` responses (RFC 8621 §4.9).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapEmailImportItemError {
    /// The message body was not a valid RFC 5322 message (RFC 8621 §4.9).
    InvalidEmail {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
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
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}

/// Failure causes during a JMAP `Email/import` flow.
#[derive(Debug, Error)]
pub enum JmapEmailImportError {
    /// The response carried no method response.
    #[error("JMAP Email/import failed: missing response in method_responses")]
    MissingResponse,
    /// The inner send coroutine failed.
    #[error("JMAP Email/import failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP Email/import failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The method response could not be parsed.
    #[error("JMAP Email/import failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    /// The server returned a method-level error.
    #[error("JMAP Email/import failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapEmailImport`].
#[derive(Clone, Debug)]
pub struct JmapEmailImportOutput {
    /// The new server state after the call.
    pub new_state: String,
    /// The created emails, keyed by client id.
    pub created: BTreeMap<String, JmapEmail>,
    /// The failed imports, keyed by client id.
    pub not_created: BTreeMap<String, JmapEmailImportItemError>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Email/import` method.
pub struct JmapEmailImport {
    state: State,
}

impl JmapEmailImport {
    /// `emails` maps client-assigned IDs to [`JmapEmailImportArgs`] descriptors.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        emails: BTreeMap<String, JmapEmailImportArgs>,
    ) -> Result<Self, JmapEmailImportError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let args = serde_json::to_value(EmailImportArgs { account_id, emails })
            .map_err(JmapEmailImportError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/import", args);
        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_MAIL_CAPABILITY.into(),
        ]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapEmailImport {
    type Yield = JmapYield;
    type Return = Result<JmapEmailImportOutput, JmapEmailImportError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapEmailImportError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<EmailImportResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailImportOutput {
                        new_state: r.new_state,
                        created: r.created,
                        not_created: r.not_created,
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapEmailImportError::ParseResponse(err)))
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
struct EmailImportArgs {
    account_id: String,
    emails: BTreeMap<String, JmapEmailImportArgs>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailImportResponse {
    new_state: String,
    #[serde(default)]
    created: BTreeMap<String, JmapEmail>,
    #[serde(default)]
    not_created: BTreeMap<String, JmapEmailImportItemError>,
}
