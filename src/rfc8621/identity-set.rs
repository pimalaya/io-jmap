//! I/O-free coroutine for the `Identity/set` method (RFC 8621 §6.4).

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    rfc8620::error::JmapMethodError,
    rfc8620::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
    rfc8621::identity::{Identity, IdentityCreate, IdentitySetError, IdentityUpdate},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapIdentitySetError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize Identity/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Identity/set response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Identity/set response in method_responses")]
    MissingResponse,
    #[error("JMAP Identity/set method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Result returned by the [`JmapIdentitySet`] coroutine.
#[derive(Debug)]
pub enum JmapIdentitySetResult {
    /// The coroutine has successfully completed.
    Ok {
        new_state: String,
        created: BTreeMap<String, Identity>,
        updated: BTreeMap<String, Option<Identity>>,
        destroyed: Vec<String>,
        not_created: BTreeMap<String, IdentitySetError>,
        not_updated: BTreeMap<String, IdentitySetError>,
        not_destroyed: BTreeMap<String, IdentitySetError>,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapIdentitySetError),
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

/// I/O-free coroutine for the JMAP `Identity/set` method.
pub struct JmapIdentitySet {
    send: JmapSend,
}

impl JmapIdentitySet {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        args: JmapIdentitySetArgs,
    ) -> Result<Self, JmapIdentitySetError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
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
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
            capabilities::SUBMISSION.into(),
        ]);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapIdentitySetResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::WantsRead => return JmapIdentitySetResult::WantsRead,
            JmapSendResult::WantsWrite(bytes) => return JmapIdentitySetResult::WantsWrite(bytes),
            JmapSendResult::Err(err) => return JmapIdentitySetResult::Err(err.into()),
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return JmapIdentitySetResult::Err(JmapIdentitySetError::MissingResponse);
        };

        if name == "error" {
            let err =
                serde_json::from_value::<JmapMethodError>(args).unwrap_or(JmapMethodError::Unknown);
            return JmapIdentitySetResult::Err(err.into());
        }

        match serde_json::from_value::<IdentitySetResponse>(args) {
            Ok(r) => JmapIdentitySetResult::Ok {
                new_state: r.new_state.unwrap_or_default(),
                created: BTreeMap::new(),
                updated: BTreeMap::new(),
                destroyed: r.destroyed.unwrap_or_default(),
                not_created: r.not_created.unwrap_or_default(),
                not_updated: r.not_updated.unwrap_or_default(),
                not_destroyed: r.not_destroyed.unwrap_or_default(),
                keep_alive,
            },
            Err(err) => JmapIdentitySetResult::Err(JmapIdentitySetError::ParseResponse(err)),
        }
    }
}
