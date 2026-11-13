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
        session::{JmapSession, JmapSessionAccountError},
        set::SetResponse,
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

/// Per-object error returned in `Email/import` responses (RFC 8621 §4.8).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapEmailImportItemError {
    /// The server forbids duplicates and the email already exists in the
    /// account (RFC 8621 §4.8).
    AlreadyExists {
        /// The id of the existing email.
        #[serde(rename = "existingId")]
        existing_id: String,
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The message body was not a valid RFC 5322 message (RFC 8621 §4.8).
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

impl fmt::Display for JmapEmailImportItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyExists {
                existing_id,
                description,
            } => {
                write!(f, "JMAP alreadyExists (existing id {existing_id})")?;
                if let Some(description) = description {
                    write!(f, ": {description}")?;
                }
                Ok(())
            }
            Self::InvalidEmail { description } => write_described(f, "invalidEmail", description),
            Self::NotFound { description } => write_described(f, "notFound", description),
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

/// Failure causes during a JMAP `Email/import` flow.
#[derive(Debug, Error)]
pub enum JmapEmailImportError {
    /// The session names no account to address the call to.
    #[error("JMAP Email/import failed: {0}")]
    Account(#[from] JmapSessionAccountError),
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
        let account_id = String::from(session.account_for(JMAP_MAIL_CAPABILITY)?);
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
                        .unwrap_or_else(JmapMethodError::unreadable);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<SetResponse<JmapEmail, JmapEmailImportItemError>>(
                    args,
                ) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailImportOutput {
                        new_state: r.new_state.unwrap_or_default(),
                        created: r.created.unwrap_or_default(),
                        not_created: r.not_created.unwrap_or_default(),
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

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString};

    use super::*;

    /// Runs one `Email/import` of `m0` against the response `body`.
    fn import(body: &[u8]) -> JmapEmailImportOutput {
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
        let import = JmapEmailImportArgs {
            blob_id: "b1".to_string(),
            mailbox_ids: BTreeMap::from([("drafts".to_string(), true)]),
            keywords: None,
            received_at: None,
        };
        let emails = BTreeMap::from([("m0".to_string(), import)]);
        let mut coroutine =
            JmapEmailImport::new(&session, &SecretString::from("Bearer t"), emails).unwrap();

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
        let JmapCoroutineState::Complete(Ok(out)) = coroutine.resume(Some(&reply)) else {
            panic!("the import did not complete");
        };

        out
    }

    /// Fastmail answers `notCreated: null` when every import landed, which
    /// RFC 8620 §5.3 allows of a create map.
    #[test]
    fn a_null_map_reads_as_empty() {
        let out = import(
            br#"{
                "methodResponses": [["Email/import", {
                    "accountId": "a1", "oldState": "e1", "newState": "e2",
                    "created": {"m0": {"id": "E1", "blobId": "b1"}}, "notCreated": null
                }, "c0"]],
                "sessionState": "s1"
            }"#,
        );

        assert!(out.not_created.is_empty());
        assert_eq!(out.created["m0"].id.as_deref(), Some("E1"));
    }

    /// Fastmail refuses an import of a message the account holds with
    /// `alreadyExists`, naming the existing email (RFC 8621 §4.8).
    #[test]
    fn a_duplicate_names_the_existing_email() {
        let out = import(
            br#"{
                "methodResponses": [["Email/import", {
                    "accountId": "a1", "oldState": "e1", "newState": "e1",
                    "created": null,
                    "notCreated": {"m0": {"type": "alreadyExists", "existingId": "E1"}}
                }, "c0"]],
                "sessionState": "s1"
            }"#,
        );

        assert!(matches!(
            &out.not_created["m0"],
            JmapEmailImportItemError::AlreadyExists { existing_id, .. } if existing_id == "E1"
        ));
        assert_eq!(
            out.not_created["m0"].to_string(),
            "JMAP alreadyExists (existing id E1)"
        );
    }

    #[test]
    fn an_unmodelled_error_keeps_its_type_and_description() {
        let out = import(
            br#"{
                "methodResponses": [["Email/import", {
                    "accountId": "a1", "oldState": "e1", "newState": "e1",
                    "notCreated": {"m0": {"type": "overQuota", "description": "full"}}
                }, "c0"]],
                "sessionState": "s1"
            }"#,
        );

        assert!(matches!(
            &out.not_created["m0"],
            JmapEmailImportItemError::Other { type_name, description }
                if type_name == "overQuota" && description.as_deref() == Some("full")
        ));
        assert_eq!(out.not_created["m0"].to_string(), "JMAP overQuota: full");
    }
}
