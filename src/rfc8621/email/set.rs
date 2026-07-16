//! JMAP `Email/set` coroutine (RFC 8621 §4.7): wraps the generic [`JmapSet`]
//! with [`JmapEmailSetArgs`] (create/update/destroy) and decodes per-object
//! [`JmapEmailSetItemError`] payloads.
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
//!     rfc8621::email::set::{JmapEmailSet, JmapEmailSetArgs},
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
//! let mut args = JmapEmailSetArgs::default();
//! args.destroy("e1");
//! let mut coroutine = JmapEmailSet::new(&session, &auth, args).unwrap();
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

use alloc::{collections::BTreeMap, format, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{JMAP_CORE_CAPABILITY, request::JmapBatch, send::*, session::JmapSession, set::*},
    rfc8621::{JMAP_MAIL_CAPABILITY, email::JmapEmail},
};

/// A single operation in an `Email/set` update patch (RFC 8621 §4.7). Each
/// variant serialises as a JSON Pointer entry in a flat patch object.
#[derive(Clone, Debug)]
pub enum JmapEmailPatchOp {
    /// Set a keyword: `"keywords/<kw>": true`
    SetKeyword(String),
    /// Unset a keyword: `"keywords/<kw>": null`
    UnsetKeyword(String),
    /// Replace all keywords atomically: `"keywords": { ... }`
    ReplaceKeywords(BTreeMap<String, bool>),
    /// Add email to a mailbox: `"mailboxIds/<id>": true`
    AddToMailbox(String),
    /// Remove email from a mailbox: `"mailboxIds/<id>": null`
    RemoveFromMailbox(String),
    /// Replace mailbox membership atomically: `"mailboxIds": { ... }`
    ReplaceMailboxIds(BTreeMap<String, bool>),
}

/// A set of patch operations applied to a single email in `Email/set`.
///
/// Serializes to a flat JSON Merge Patch object (RFC 7396).
#[derive(Clone, Debug, Default)]
pub struct JmapEmailPatch(pub Vec<JmapEmailPatchOp>);

impl JmapEmailPatch {
    /// Appends a [`JmapEmailPatchOp::SetKeyword`] operation.
    pub fn set_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::SetKeyword(keyword.into()));
        self
    }

    /// Appends a [`JmapEmailPatchOp::UnsetKeyword`] operation.
    pub fn unset_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::UnsetKeyword(keyword.into()));
        self
    }

    /// Appends a [`JmapEmailPatchOp::ReplaceKeywords`] operation.
    pub fn replace_keywords(mut self, keywords: BTreeMap<String, bool>) -> Self {
        self.0.push(JmapEmailPatchOp::ReplaceKeywords(keywords));
        self
    }

    /// Appends a [`JmapEmailPatchOp::AddToMailbox`] operation.
    pub fn add_to_mailbox(mut self, id: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::AddToMailbox(id.into()));
        self
    }

    /// Appends a [`JmapEmailPatchOp::RemoveFromMailbox`] operation.
    pub fn remove_from_mailbox(mut self, id: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::RemoveFromMailbox(id.into()));
        self
    }

    /// Appends a [`JmapEmailPatchOp::ReplaceMailboxIds`] operation.
    pub fn replace_mailbox_ids(mut self, ids: BTreeMap<String, bool>) -> Self {
        self.0.push(JmapEmailPatchOp::ReplaceMailboxIds(ids));
        self
    }
}

impl Serialize for JmapEmailPatch {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for op in &self.0 {
            match op {
                JmapEmailPatchOp::SetKeyword(kw) => {
                    map.serialize_entry(&format!("keywords/{kw}"), &true)?
                }
                JmapEmailPatchOp::UnsetKeyword(kw) => {
                    map.serialize_entry(&format!("keywords/{kw}"), &Option::<bool>::None)?
                }
                JmapEmailPatchOp::ReplaceKeywords(kws) => map.serialize_entry("keywords", kws)?,
                JmapEmailPatchOp::AddToMailbox(id) => {
                    map.serialize_entry(&format!("mailboxIds/{id}"), &true)?
                }
                JmapEmailPatchOp::RemoveFromMailbox(id) => {
                    map.serialize_entry(&format!("mailboxIds/{id}"), &Option::<bool>::None)?
                }
                JmapEmailPatchOp::ReplaceMailboxIds(ids) => {
                    map.serialize_entry("mailboxIds", ids)?
                }
            }
        }
        map.end()
    }
}

/// Per-object error returned in `Email/set` responses (RFC 8621 §4.7).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapEmailSetItemError {
    /// The email would exceed the server's keyword limit (RFC 8621 §4.7).
    TooManyKeywords {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The email would be in too many mailboxes (RFC 8621 §4.7).
    TooManyMailboxes {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// One or more blob IDs in the email were not found (RFC 8621 §4.7).
    BlobNotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): patch could not be applied.
    InvalidPatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): would destroy an object already
    /// queued for destruction in the same request.
    WillDestroy {
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
    /// Standard set error (RFC 8620 §5.3): tried to create/destroy a
    /// server-managed singleton.
    Singleton {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}

/// Failure causes during a JMAP `Email/set` flow.
#[derive(Debug, Error)]
pub enum JmapEmailSetError {
    /// The inner send coroutine failed.
    #[error("JMAP Email/set failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP Email/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The inner generic set coroutine failed.
    #[error("JMAP Email/set failed: {0}")]
    Set(#[from] JmapSetError),
}

/// Arguments for an `Email/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailSetArgs {
    /// Objects to create (client ID → partial email object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, JmapEmail>>,
    /// Objects to update (email ID → patch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, JmapEmailPatch>>,
    /// IDs to destroy (delete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,
}

impl JmapEmailSetArgs {
    /// Queue an email for creation under the given client-chosen ID.
    pub fn create(&mut self, client_id: impl Into<String>, email: JmapEmail) -> &mut Self {
        self.create
            .get_or_insert_with(Default::default)
            .insert(client_id.into(), email);
        self
    }

    /// Queue an email ID for destruction.
    pub fn destroy(&mut self, id: impl Into<String>) -> &mut Self {
        self.destroy
            .get_or_insert_with(Default::default)
            .push(id.into());
        self
    }

    /// Queues a patch setting a keyword on the email with the given id.
    pub fn set_keyword(&mut self, id: impl Into<String>, keyword: impl Into<String>) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::SetKeyword(keyword.into()));
        self
    }

    /// Queues a patch unsetting a keyword on the email with the given id.
    pub fn unset_keyword(
        &mut self,
        id: impl Into<String>,
        keyword: impl Into<String>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::UnsetKeyword(keyword.into()));
        self
    }

    /// Queues a patch replacing all keywords of the email with the given id.
    pub fn replace_keywords(
        &mut self,
        id: impl Into<String>,
        keywords: BTreeMap<String, bool>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::ReplaceKeywords(keywords));
        self
    }

    /// Queues a patch adding the email with the given id to a mailbox.
    pub fn add_to_mailbox(
        &mut self,
        id: impl Into<String>,
        mailbox_id: impl Into<String>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::AddToMailbox(mailbox_id.into()));
        self
    }

    /// Queues a patch removing the email with the given id from a mailbox.
    pub fn remove_from_mailbox(
        &mut self,
        id: impl Into<String>,
        mailbox_id: impl Into<String>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::RemoveFromMailbox(mailbox_id.into()));
        self
    }

    /// Queues a patch replacing the mailbox membership of the email with
    /// the given id.
    pub fn replace_mailbox_ids(
        &mut self,
        id: impl Into<String>,
        ids: BTreeMap<String, bool>,
    ) -> &mut Self {
        self.patch(id)
            .0
            .push(JmapEmailPatchOp::ReplaceMailboxIds(ids));
        self
    }

    fn patch(&mut self, id: impl Into<String>) -> &mut JmapEmailPatch {
        self.update
            .get_or_insert_with(Default::default)
            .entry(id.into())
            .or_default()
    }
}

/// Successful terminal output of [`JmapEmailSet`].
#[derive(Clone, Debug)]
pub struct JmapEmailSetOutput {
    /// The new server state after the call.
    pub new_state: String,
    /// The created emails, keyed by client id.
    pub created: BTreeMap<String, JmapEmail>,
    /// The updated emails, keyed by id.
    pub updated: BTreeMap<String, Option<JmapEmail>>,
    /// Ids of the destroyed objects.
    pub destroyed: Vec<String>,
    /// The failed creates, keyed by client id.
    pub not_created: BTreeMap<String, JmapEmailSetItemError>,
    /// The failed updates, keyed by id.
    pub not_updated: BTreeMap<String, JmapEmailSetItemError>,
    /// The failed destroys, keyed by id.
    pub not_destroyed: BTreeMap<String, JmapEmailSetItemError>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Email/set` method.
pub struct JmapEmailSet {
    state: State,
}

impl JmapEmailSet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapEmailSetArgs,
    ) -> Result<Self, JmapEmailSetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(EmailSetRequest { account_id, args })
            .map_err(JmapEmailSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Email/set", json_args);
        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_MAIL_CAPABILITY.into(),
        ]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            state: State::Set(JmapSet::from_send(send)),
        })
    }
}

impl JmapCoroutine for JmapEmailSet {
    type Yield = JmapYield;
    type Return = Result<JmapEmailSetOutput, JmapEmailSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Set(set) => {
                let JmapSetOutput {
                    new_state,
                    created,
                    updated,
                    destroyed,
                    not_created,
                    not_updated,
                    not_destroyed,
                    keep_alive,
                } = jmap_try!(set, arg);
                let parse = |map: BTreeMap<String, serde_json::Value>| {
                    map.into_iter()
                        .map(|(k, v)| {
                            let e =
                                serde_json::from_value(v).unwrap_or(JmapEmailSetItemError::Unknown);
                            (k, e)
                        })
                        .collect()
                };
                JmapCoroutineState::Complete(Ok(JmapEmailSetOutput {
                    new_state,
                    created,
                    updated,
                    destroyed,
                    not_created: parse(not_created),
                    not_updated: parse(not_updated),
                    not_destroyed: parse(not_destroyed),
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Set(JmapSet<JmapEmail>),
}

#[derive(Serialize)]
struct EmailSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: JmapEmailSetArgs,
}
