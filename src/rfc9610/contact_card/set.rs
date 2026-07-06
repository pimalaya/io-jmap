//! JMAP `ContactCard/set` coroutine (RFC 9610 §3.5): wraps the generic
//! [`JmapSet`] with [`JmapContactCardSetArgs`] (create/update/destroy) and
//! decodes per-object [`JmapContactCardSetItemError`] payloads.
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
//!     rfc8620::JmapSession,
//!     rfc9610::contact_card::set::{JmapContactCardSet, JmapContactCardSetArgs},
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
//!     JmapContactCardSet::new(&session, &auth, JmapContactCardSetArgs::default()).unwrap();
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

use log::trace;
use secrecy::SecretString;
use serde::Serialize;
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{CORE_CAPABILITY, JmapBatch, JmapSession, send::*, set::*},
    rfc9610::{
        CONTACTS_CAPABILITY,
        contact_card::{JmapContactCard, JmapContactCardPatch, JmapContactCardSetItemError},
    },
};

/// Failure causes during a JMAP `ContactCard/set` flow.
#[derive(Debug, Error)]
pub enum JmapContactCardSetError {
    #[error("JMAP ContactCard/set failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP ContactCard/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP ContactCard/set failed: {0}")]
    Set(#[from] JmapSetError),
}

/// Arguments for a `ContactCard/set` request.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapContactCardSetArgs {
    /// Objects to create (client ID → ContactCard object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<BTreeMap<String, JmapContactCard>>,

    /// Objects to update (ContactCard ID → patch object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<BTreeMap<String, JmapContactCardPatch>>,

    /// IDs of objects to destroy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroy: Option<Vec<String>>,
}

/// Successful terminal output of [`JmapContactCardSet`].
#[derive(Clone, Debug)]
pub struct JmapContactCardSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, JmapContactCard>,
    pub updated: BTreeMap<String, Option<JmapContactCard>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, JmapContactCardSetItemError>,
    pub not_updated: BTreeMap<String, JmapContactCardSetItemError>,
    pub not_destroyed: BTreeMap<String, JmapContactCardSetItemError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `ContactCard/set` method.
pub struct JmapContactCardSet {
    state: State,
}

impl JmapContactCardSet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapContactCardSetArgs,
    ) -> Result<Self, JmapContactCardSetError> {
        let account_id = session
            .primary_accounts
            .get(CONTACTS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(ContactCardSetRequest { account_id, args })
            .map_err(JmapContactCardSetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("ContactCard/set", json_args);
        let request = batch.into_request(vec![CORE_CAPABILITY.into(), CONTACTS_CAPABILITY.into()]);

        let send = JmapSend::new(http_auth, api_url, request)?;
        Ok(Self {
            state: State::Set(JmapSet::from_send(send)),
        })
    }
}

impl JmapCoroutine for JmapContactCardSet {
    type Yield = JmapYield;
    type Return = Result<JmapContactCardSetOutput, JmapContactCardSetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("ContactCard/set: {}", self.state);
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
                                .unwrap_or(JmapContactCardSetItemError::Unknown);
                            (k, e)
                        })
                        .collect()
                };
                JmapCoroutineState::Complete(Ok(JmapContactCardSetOutput {
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
    Set(JmapSet<JmapContactCard>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Set(_) => f.write_str("set"),
        }
    }
}

#[derive(Serialize)]
struct ContactCardSetRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(flatten)]
    args: JmapContactCardSetArgs,
}
