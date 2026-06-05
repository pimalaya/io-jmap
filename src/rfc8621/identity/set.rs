//! JMAP `Identity/set` coroutine (RFC 8621 §6.4): builds a custom set
//! batch (Identity has no generic `JmapSet` reuse because its set
//! response is parsed loosely to tolerate partial server output).
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::identity::set::{JmapIdentitySet, JmapIdentitySetArgs},
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let mut args = JmapIdentitySetArgs::default();
//! args.destroy("id1");
//! let coroutine = JmapIdentitySet::new(session, &auth, args).unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::coroutine::*;
use crate::jmap_try;
use crate::{
    rfc8620::{CORE_CAPABILITY, JmapBatch, JmapMethodError, JmapSession, send::*},
    rfc8621::{
        MAIL_CAPABILITY,
        email_submission::SUBMISSION_CAPABILITY,
        identity::{Identity, IdentityCreate, IdentitySetError, IdentityUpdate},
    },
};

/// Failure causes during a JMAP `Identity/set` flow.
#[derive(Debug, Error)]
pub enum JmapIdentitySetError {
    #[error("JMAP Identity/set failed: missing response in method_responses")]
    MissingResponse,
    #[error("JMAP Identity/set failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Identity/set failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP Identity/set failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP Identity/set failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Arguments for an `Identity/set` request.
#[derive(Clone, Debug, Default)]
pub struct JmapIdentitySetArgs {
    pub create: BTreeMap<String, IdentityCreate>,
    pub update: BTreeMap<String, IdentityUpdate>,
    pub destroy: Vec<String>,
}

impl JmapIdentitySetArgs {
    pub fn create(&mut self, client_id: impl Into<String>, identity: IdentityCreate) -> &mut Self {
        self.create.insert(client_id.into(), identity);
        self
    }

    pub fn update(&mut self, id: impl Into<String>, patch: IdentityUpdate) -> &mut Self {
        self.update.insert(id.into(), patch);
        self
    }

    pub fn destroy(&mut self, id: impl Into<String>) -> &mut Self {
        self.destroy.push(id.into());
        self
    }
}

/// Successful terminal output of [`JmapIdentitySet`].
#[derive(Clone, Debug)]
pub struct JmapIdentitySetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Identity>,
    pub updated: BTreeMap<String, Option<Identity>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, IdentitySetError>,
    pub not_updated: BTreeMap<String, IdentitySetError>,
    pub not_destroyed: BTreeMap<String, IdentitySetError>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Identity/set` method.
pub struct JmapIdentitySet {
    state: State,
}

impl JmapIdentitySet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapIdentitySetArgs,
    ) -> Result<Self, JmapIdentitySetError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
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
            CORE_CAPABILITY.into(),
            MAIL_CAPABILITY.into(),
            SUBMISSION_CAPABILITY.into(),
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
        trace!("Identity/set: {}", self.state);
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

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentitySetRequest {
    account_id: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    create: BTreeMap<String, IdentityCreate>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    update: BTreeMap<String, IdentityUpdate>,
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
    not_created: Option<BTreeMap<String, IdentitySetError>>,
    #[serde(default)]
    not_updated: Option<BTreeMap<String, IdentitySetError>>,
    #[serde(default)]
    not_destroyed: Option<BTreeMap<String, IdentitySetError>>,
}
