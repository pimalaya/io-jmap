//! JMAP `Identity/get` coroutine (RFC 8621 §6.3): wraps the generic [`JmapGet`]
//! with the Submission capability.
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
//!     rfc8621::identity::get::{JmapIdentityGet, JmapIdentityGetOptions},
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
//!     JmapIdentityGet::new(&session, &auth, JmapIdentityGetOptions::default()).unwrap();
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
//! println!("{} identities", out.identities.len());
//! ```

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{JMAP_CORE_CAPABILITY, JmapSession, get::*},
    rfc8621::{
        JMAP_MAIL_CAPABILITY, email_submission::JMAP_SUBMISSION_CAPABILITY, identity::JmapIdentity,
    },
};

/// Failure causes during a JMAP `Identity/get` flow.
#[derive(Debug, Error)]
pub enum JmapIdentityGetError {
    /// The inner generic get coroutine failed.
    #[error("JMAP Identity/get failed: {0}")]
    Get(#[from] JmapGetError),
}

/// Options for [`JmapIdentityGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapIdentityGetOptions {
    /// Restrict the fetch to these identity IDs; `None` fetches all.
    pub ids: Option<Vec<String>>,
}

/// Successful terminal output of [`JmapIdentityGet`].
#[derive(Clone, Debug)]
pub struct JmapIdentityGetOutput {
    /// The fetched identities.
    pub identities: Vec<JmapIdentity>,
    /// The requested ids the server did not find.
    pub not_found: Vec<String>,
    /// The new server state after the call.
    pub new_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Identity/get` method.
pub struct JmapIdentityGet {
    state: State,
}

impl JmapIdentityGet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapIdentityGetOptions,
    ) -> Result<Self, JmapIdentityGetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            state: State::Get(JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "Identity/get",
                vec![
                    JMAP_CORE_CAPABILITY.into(),
                    JMAP_MAIL_CAPABILITY.into(),
                    JMAP_SUBMISSION_CAPABILITY.into(),
                ],
                JmapGetOptions {
                    ids: opts.ids,
                    properties: None,
                },
            )?),
        })
    }
}

impl JmapCoroutine for JmapIdentityGet {
    type Yield = JmapYield;
    type Return = Result<JmapIdentityGetOutput, JmapIdentityGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Get(get) => {
                let JmapGetOutput {
                    list,
                    not_found,
                    state,
                    keep_alive,
                } = jmap_try!(get, arg);
                JmapCoroutineState::Complete(Ok(JmapIdentityGetOutput {
                    identities: list,
                    not_found,
                    new_state: state,
                    keep_alive,
                }))
            }
        }
    }
}

enum State {
    Get(JmapGet<JmapIdentity>),
}
