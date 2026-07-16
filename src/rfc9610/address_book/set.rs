//! JMAP `AddressBook/set` coroutine (RFC 9610 §2.3): wraps the generic
//! [`JmapSet`] with [`JmapAddressBookSetArgs`] (create/update/destroy plus
//! the `onDestroyRemoveContents` and `onSuccessSetIsDefault` extra
//! arguments) and decodes per-object [`JmapAddressBookSetItemError`]
//! payloads.
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
//!     rfc9610::address_book::set::{JmapAddressBookSet, JmapAddressBookSetArgs},
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
//!     JmapAddressBookSet::new(&session, &auth, JmapAddressBookSetArgs::default()).unwrap();
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
    rfc9610::{
        JMAP_CONTACTS_CAPABILITY,
        address_book::{JmapAddressBook, JmapAddressBookRights},
    },
};

/// Client-settable subset of [`JmapAddressBook`] for `AddressBook/set`
/// create requests (RFC 9610 §2). Server-assigned fields are excluded.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAddressBookCreate {
    /// The user-visible address book name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional long-form description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the address book.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
    /// Principal id to rights map (RFC 9670).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_with: Option<BTreeMap<String, JmapAddressBookRights>>,
}

/// Patch object for `AddressBook/set` update requests (RFC 8620 §5.3): only
/// `Some` fields are serialised.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAddressBookUpdate {
    /// The user-visible address book name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional long-form description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Position hint for display ordering (lower first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<u32>,
    /// Whether the user is subscribed to the address book.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
    /// Principal id to rights map (RFC 9670).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_with: Option<BTreeMap<String, JmapAddressBookRights>>,
}

/// Per-object error returned in `AddressBook/set` responses (RFC 9610 §2.3).
///
/// Covers the standard RFC 8620 §5.3 set errors plus the AddressBook-specific
/// error defined in RFC 9610 §2.3.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapAddressBookSetItemError {
    /// The AddressBook still has ContactCards and `onDestroyRemoveContents`
    /// was false (RFC 9610 §2.3).
    AddressBookHasContents {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): the change is not allowed, e.g.
    /// a `shareWith` or `isSubscribed` change rejected by the server.
    Forbidden {
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

/// Failure causes during a JMAP `AddressBook/set` flow.
#[derive(Debug, Error)]
pub enum JmapAddressBookSetError {
    /// The inner send coroutine failed.
    #[error("JMAP AddressBook/set failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP AddressBook/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The inner generic set coroutine failed.
    #[error("JMAP AddressBook/set failed: {0}")]
    Set(#[from] JmapSetError),
}

/// Arguments for an `AddressBook/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapAddressBookSetArgs {
    /// Objects to create (client ID → partial AddressBook object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, JmapAddressBookCreate>>,
    /// Objects to update (AddressBook ID → patch object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, JmapAddressBookUpdate>>,
    /// IDs of objects to destroy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,
    /// Whether to remove contained ContactCards when destroying an
    /// AddressBook; a card left in no AddressBook is destroyed
    /// (RFC 9610 §2.3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_destroy_remove_contents: Option<bool>,
    /// AddressBook ID (or `#`-prefixed creation ID) to make the default
    /// when all changes succeed (RFC 9610 §2.3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_success_set_is_default: Option<String>,
}

/// Successful terminal output of [`JmapAddressBookSet`].
#[derive(Clone, Debug)]
pub struct JmapAddressBookSetOutput {
    /// The new server state after the call.
    pub new_state: String,
    /// The created address books, keyed by client id.
    pub created: BTreeMap<String, JmapAddressBook>,
    /// The updated address books, keyed by id.
    pub updated: BTreeMap<String, Option<JmapAddressBook>>,
    /// Ids of the destroyed objects.
    pub destroyed: Vec<String>,
    /// The failed creates, keyed by client id.
    pub not_created: BTreeMap<String, JmapAddressBookSetItemError>,
    /// The failed updates, keyed by id.
    pub not_updated: BTreeMap<String, JmapAddressBookSetItemError>,
    /// The failed destroys, keyed by id.
    pub not_destroyed: BTreeMap<String, JmapAddressBookSetItemError>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `AddressBook/set` method.
pub struct JmapAddressBookSet {
    state: State,
}

impl JmapAddressBookSet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapAddressBookSetArgs,
    ) -> Result<Self, JmapAddressBookSetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_CONTACTS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(AddressBookSetRequest { account_id, args })
            .map_err(JmapAddressBookSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("AddressBook/set", json_args);
        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_CONTACTS_CAPABILITY.into(),
        ]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            state: State::Set(JmapSet::from_send(send)),
        })
    }
}

impl JmapCoroutine for JmapAddressBookSet {
    type Yield = JmapYield;
    type Return = Result<JmapAddressBookSetOutput, JmapAddressBookSetError>;

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
                                .unwrap_or(JmapAddressBookSetItemError::Unknown);
                            (k, e)
                        })
                        .collect()
                };
                JmapCoroutineState::Complete(Ok(JmapAddressBookSetOutput {
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
    Set(JmapSet<JmapAddressBook>),
}

#[derive(Serialize)]
struct AddressBookSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: JmapAddressBookSetArgs,
}
