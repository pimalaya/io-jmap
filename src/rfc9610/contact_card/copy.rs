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
//!     rfc8620::JmapSession,
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

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec};

use log::trace;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{CORE_CAPABILITY, JmapBatch, JmapMethodError, JmapSession, send::*},
    rfc9610::{
        CONTACTS_CAPABILITY,
        contact_card::{JmapContactCard, JmapContactCardCopyArgs, JmapContactCardCopyItemError},
    },
};

/// Failure causes during a JMAP `ContactCard/copy` flow.
#[derive(Debug, Error)]
pub enum JmapContactCardCopyError {
    #[error("JMAP ContactCard/copy failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP ContactCard/copy failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP ContactCard/copy failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP ContactCard/copy failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP ContactCard/copy failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Successful terminal output of [`JmapContactCardCopy`].
#[derive(Clone, Debug)]
pub struct JmapContactCardCopyOutput {
    pub new_state: String,
    pub created: BTreeMap<String, JmapContactCard>,
    pub not_created: BTreeMap<String, JmapContactCardCopyItemError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `ContactCard/copy` method.
pub struct JmapContactCardCopy {
    state: State,
}

impl JmapContactCardCopy {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        from_account_id: impl Into<String>,
        cards: BTreeMap<String, JmapContactCardCopyArgs>,
    ) -> Result<Self, JmapContactCardCopyError> {
        let account_id = session
            .primary_accounts
            .get(CONTACTS_CAPABILITY)
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
        let request = batch.into_request(vec![CORE_CAPABILITY.into(), CONTACTS_CAPABILITY.into()]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapContactCardCopy {
    type Yield = JmapYield;
    type Return = Result<JmapContactCardCopyOutput, JmapContactCardCopyError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("ContactCard/copy: {}", self.state);
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

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
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
