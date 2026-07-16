//! JMAP `Identity/set` coroutine (RFC 8621 §6.4): builds a custom set batch
//! (Identity has no generic `JmapSet` reuse because its set response is parsed
//! loosely to tolerate partial server output).
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
//!     rfc8621::identity::set::{JmapIdentitySet, JmapIdentitySetArgs},
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
//! let mut args = JmapIdentitySetArgs::default();
//! args.destroy("id1");
//! let mut coroutine = JmapIdentitySet::new(&session, &auth, args).unwrap();
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
    rfc8620::{
        JMAP_CORE_CAPABILITY, error::JmapMethodError, request::JmapBatch, send::*,
        session::JmapSession,
    },
    rfc8621::{
        JMAP_MAIL_CAPABILITY, email::JmapEmailAddress,
        email_submission::JMAP_SUBMISSION_CAPABILITY, identity::JmapIdentity,
    },
};

/// A partial [`JmapIdentity`] object for `Identity/set` create requests.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapIdentityCreate {
    /// The display name for the sender.
    pub name: String,
    /// The email address for the sender.
    pub email: String,
    /// `Reply-To` addresses to set on outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Vec<JmapEmailAddress>>,
    /// `Bcc` addresses to add to all outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<Vec<JmapEmailAddress>>,
    /// Plaintext signature to append to outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,
    /// HTML signature to append to outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_signature: Option<String>,
}

/// Patch object for `Identity/set` update requests.
///
/// Only `Some` fields are serialized.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapIdentityUpdate {
    /// The display name for the sender.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `Reply-To` addresses to set on outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Vec<JmapEmailAddress>>,
    /// `Bcc` addresses to add to all outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<Vec<JmapEmailAddress>>,
    /// Plaintext signature to append to outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,
    /// HTML signature to append to outgoing email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_signature: Option<String>,
}

/// Per-object error returned in `Identity/set` responses (RFC 8621 §6.4).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapIdentitySetItemError {
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

/// Failure causes during a JMAP `Identity/set` flow.
#[derive(Debug, Error)]
pub enum JmapIdentitySetError {
    /// The response carried no method response.
    #[error("JMAP Identity/set failed: missing response in method_responses")]
    MissingResponse,
    /// The inner send coroutine failed.
    #[error("JMAP Identity/set failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP Identity/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The method response could not be parsed.
    #[error("JMAP Identity/set failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    /// The server returned a method-level error.
    #[error("JMAP Identity/set failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Arguments for an `Identity/set` request.
#[derive(Clone, Debug, Default)]
pub struct JmapIdentitySetArgs {
    /// The identities to create, keyed by client id.
    pub create: BTreeMap<String, JmapIdentityCreate>,
    /// The patches to apply, keyed by identity id.
    pub update: BTreeMap<String, JmapIdentityUpdate>,
    /// The ids of the objects to destroy.
    pub destroy: Vec<String>,
}

impl JmapIdentitySetArgs {
    /// Queues an object to create under the given client id.
    pub fn create(
        &mut self,
        client_id: impl Into<String>,
        identity: JmapIdentityCreate,
    ) -> &mut Self {
        self.create.insert(client_id.into(), identity);
        self
    }

    /// Queues a patch for the identity with the given id.
    pub fn update(&mut self, id: impl Into<String>, patch: JmapIdentityUpdate) -> &mut Self {
        self.update.insert(id.into(), patch);
        self
    }

    /// Queues the object with the given id for destruction.
    pub fn destroy(&mut self, id: impl Into<String>) -> &mut Self {
        self.destroy.push(id.into());
        self
    }
}

/// Successful terminal output of [`JmapIdentitySet`].
#[derive(Clone, Debug)]
pub struct JmapIdentitySetOutput {
    /// The new server state after the call.
    pub new_state: String,
    /// The created identities, keyed by client id.
    pub created: BTreeMap<String, JmapIdentity>,
    /// The updated identities, keyed by id.
    pub updated: BTreeMap<String, Option<JmapIdentity>>,
    /// Ids of the destroyed objects.
    pub destroyed: Vec<String>,
    /// The failed creates, keyed by client id.
    pub not_created: BTreeMap<String, JmapIdentitySetItemError>,
    /// The failed updates, keyed by id.
    pub not_updated: BTreeMap<String, JmapIdentitySetItemError>,
    /// The failed destroys, keyed by id.
    pub not_destroyed: BTreeMap<String, JmapIdentitySetItemError>,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Identity/set` method.
pub struct JmapIdentitySet {
    state: State,
}

impl JmapIdentitySet {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapIdentitySetArgs,
    ) -> Result<Self, JmapIdentitySetError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let json_args = serde_json::to_value(IdentitySetRequest {
            account_id,
            create: args.create,
            update: args.update,
            destroy: args.destroy,
        })
        .map_err(JmapIdentitySetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Identity/set", json_args);
        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_MAIL_CAPABILITY.into(),
            JMAP_SUBMISSION_CAPABILITY.into(),
        ]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapIdentitySet {
    type Yield = JmapYield;
    type Return = Result<JmapIdentitySetOutput, JmapIdentitySetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapIdentitySetError::MissingResponse,
                    ));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<IdentitySetResponse>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapIdentitySetOutput {
                        new_state: r.new_state.unwrap_or_default(),
                        created: BTreeMap::new(),
                        updated: BTreeMap::new(),
                        destroyed: r.destroyed.unwrap_or_default(),
                        not_created: r.not_created.unwrap_or_default(),
                        not_updated: r.not_updated.unwrap_or_default(),
                        not_destroyed: r.not_destroyed.unwrap_or_default(),
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapIdentitySetError::ParseResponse(err)))
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
struct IdentitySetRequest {
    account_id: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    create: BTreeMap<String, JmapIdentityCreate>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    update: BTreeMap<String, JmapIdentityUpdate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    destroy: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentitySetResponse {
    #[serde(default)]
    new_state: Option<String>,
    /// Parsed as raw JSON to avoid issues with partial server responses.
    #[serde(default)]
    #[allow(dead_code)]
    created: Option<serde_json::Value>,
    /// Parsed as raw JSON to avoid issues with partial server responses.
    #[serde(default)]
    #[allow(dead_code)]
    updated: Option<serde_json::Value>,
    #[serde(default)]
    destroyed: Option<Vec<String>>,
    #[serde(default)]
    not_created: Option<BTreeMap<String, JmapIdentitySetItemError>>,
    #[serde(default)]
    not_updated: Option<BTreeMap<String, JmapIdentitySetItemError>>,
    #[serde(default)]
    not_destroyed: Option<BTreeMap<String, JmapIdentitySetItemError>>,
}
