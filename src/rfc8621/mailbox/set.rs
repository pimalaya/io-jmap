//! JMAP `Mailbox/set` coroutine (RFC 8621 §2.6): wraps the generic [`JmapSet`]
//! with [`JmapMailboxSetArgs`] (create/update/destroy) and decodes per-object
//! [`JmapMailboxSetItemError`] payloads.
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
//!     rfc8621::mailbox::set::{JmapMailboxSet, JmapMailboxSetArgs},
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
//! let mut coroutine =
//!     JmapMailboxSet::new(&session, &auth, JmapMailboxSetArgs::default()).unwrap();
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

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{JMAP_CORE_CAPABILITY, request::JmapBatch, send::*, session::JmapSession, set::*},
    rfc8621::{
        JMAP_MAIL_CAPABILITY,
        mailbox::{JmapMailbox, JmapMailboxRole},
    },
};

/// Client-settable subset of [`JmapMailbox`] for `Mailbox/set` create requests
/// (RFC 8621 §2.1). Server-assigned fields are excluded.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxCreate {
    /// The user-visible mailbox name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The parent mailbox id; `None` for a top-level mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// The special-use role of the mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<JmapMailboxRole>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
}

/// Patch object for `Mailbox/set` update requests (RFC 8620 §5.3): only
/// `Some` fields are serialised.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxUpdate {
    /// The user-visible mailbox name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The parent mailbox id; `None` for a top-level mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// The special-use role of the mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<JmapMailboxRole>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
}

/// Per-object error returned in `Mailbox/set` responses (RFC 8621 §2.6).
///
/// Covers the standard RFC 8620 §5.3 set errors plus the mailbox-specific
/// errors defined in RFC 8621 §2.6.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapMailboxSetItemError {
    /// The mailbox cannot be destroyed because it has child mailboxes.
    MailboxHasChild {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The mailbox cannot be destroyed because it contains email.
    MailboxHasEmail {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The referenced object does not exist.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The update patch is invalid.
    InvalidPatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The object will be destroyed by this request, so it cannot be updated.
    WillDestroy {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// One or more object properties are invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// The type is a singleton, objects cannot be created or destroyed.
    Singleton {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Any error type this library does not know about.
    #[serde(other)]
    Unknown,
}

/// Failure causes during a JMAP `Mailbox/set` flow.
#[derive(Debug, Error)]
pub enum JmapMailboxSetError {
    /// The inner send coroutine failed.
    #[error("JMAP Mailbox/set failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP Mailbox/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The inner generic set coroutine failed.
    #[error("JMAP Mailbox/set failed: {0}")]
    Set(#[from] JmapSetError),
}

/// Arguments for a `Mailbox/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxSetArgs {
    /// Objects to create (client ID → partial mailbox object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, JmapMailboxCreate>>,
    /// Objects to update (mailbox ID → patch object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, JmapMailboxUpdate>>,
    /// IDs of objects to destroy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,
    /// Whether to destroy contained emails when destroying a mailbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_destroy_remove_emails: Option<bool>,
}

/// Successful terminal output of [`JmapMailboxSet`].
#[derive(Clone, Debug)]
pub struct JmapMailboxSetOutput {
    /// The new server state after the call.
    pub new_state: String,
    /// The created mailboxes, keyed by client id.
    pub created: BTreeMap<String, JmapMailbox>,
    /// The updated mailboxes, keyed by id.
    pub updated: BTreeMap<String, Option<JmapMailbox>>,
    /// Ids of the destroyed objects.
    pub destroyed: Vec<String>,
    /// The failed creates, keyed by client id.
    pub not_created: BTreeMap<String, JmapMailboxSetItemError>,
    /// The failed updates, keyed by id.
    pub not_updated: BTreeMap<String, JmapMailboxSetItemError>,
    /// The failed destroys, keyed by id.
    pub not_destroyed: BTreeMap<String, JmapMailboxSetItemError>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Mailbox/set` method.
pub struct JmapMailboxSet {
    state: State,
}

impl JmapMailboxSet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapMailboxSetArgs,
    ) -> Result<Self, JmapMailboxSetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(MailboxSetRequest { account_id, args })
            .map_err(JmapMailboxSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Mailbox/set", json_args);
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

impl JmapCoroutine for JmapMailboxSet {
    type Yield = JmapYield;
    type Return = Result<JmapMailboxSetOutput, JmapMailboxSetError>;

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
                            let e = serde_json::from_value(v)
                                .unwrap_or(JmapMailboxSetItemError::Unknown);
                            (k, e)
                        })
                        .collect()
                };
                JmapCoroutineState::Complete(Ok(JmapMailboxSetOutput {
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
    Set(JmapSet<JmapMailbox>),
}

#[derive(Serialize)]
struct MailboxSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: JmapMailboxSetArgs,
}
