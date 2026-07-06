//! JMAP `AddressBook/get` coroutine (RFC 9610 §2.1): wraps the generic
//! [`JmapGet`] with the JMAP-Contacts capability set.
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
//!     rfc9610::address_book::get::{JmapAddressBookGet, JmapAddressBookGetOptions},
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
//!     JmapAddressBookGet::new(&session, &auth, JmapAddressBookGetOptions::default()).unwrap();
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
//! println!("{} address books", out.address_books.len());
//! ```

use core::fmt;

use alloc::{borrow::ToOwned, format, string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{CORE_CAPABILITY, JmapSession, get::*},
    rfc9610::{
        CONTACTS_CAPABILITY,
        address_book::{JmapAddressBook, JmapAddressBookProperty},
    },
};

/// Failure causes during a JMAP `AddressBook/get` flow.
#[derive(Debug, Error)]
pub enum JmapAddressBookGetError {
    #[error("JMAP AddressBook/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Options for [`JmapAddressBookGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapAddressBookGetOptions {
    /// Restrict the fetch to these AddressBook IDs; `None` fetches all.
    pub ids: Option<Vec<String>>,
    /// Restrict the returned properties; `None` returns all.
    pub properties: Option<Vec<JmapAddressBookProperty>>,
}

/// Successful terminal output of [`JmapAddressBookGet`].
#[derive(Clone, Debug)]
pub struct JmapAddressBookGetOutput {
    pub address_books: Vec<JmapAddressBook>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `AddressBook/get` method.
pub struct JmapAddressBookGet {
    state: State,
}

impl JmapAddressBookGet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapAddressBookGetOptions,
    ) -> Result<Self, JmapAddressBookGetError> {
        let account_id = session
            .primary_accounts
            .get(CONTACTS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let props = opts.properties.map(|ps| {
            ps.iter()
                .map(|p| {
                    serde_json::to_value(p)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_else(|| format!("{p:?}"))
                })
                .collect()
        });

        Ok(Self {
            state: State::Get(JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "AddressBook/get",
                vec![CORE_CAPABILITY.into(), CONTACTS_CAPABILITY.into()],
                JmapGetOptions {
                    ids: opts.ids,
                    properties: props,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapAddressBookGet {
    type Yield = JmapYield;
    type Return = Result<JmapAddressBookGetOutput, JmapAddressBookGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("AddressBook/get: {}", self.state);
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapAddressBookGetOutput {
                    address_books: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<JmapAddressBook>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get(_) => f.write_str("get"),
        }
    }
}
