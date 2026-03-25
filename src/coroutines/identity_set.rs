//! I/O-free coroutine for the `Identity/set` method (RFC 8621 §6.4).

use std::collections::HashMap;

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        error::JmapMethodError,
        identity::{Identity, IdentityCreate, IdentityUpdate},
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum SetJmapIdentitiesError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Identity/set args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Identity/set response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("Missing Identity/set response in method_responses")]
    MissingResponse,
    #[error("JMAP Identity/set method error: {0}")]
    Method(#[from] JmapMethodError),
}

/// Per-object set error.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<String>,
}

/// Result returned by the [`SetJmapIdentities`] coroutine.
#[derive(Debug)]
pub enum SetJmapIdentitiesResult {
    Ok {
        context: JmapContext,
        new_state: String,
        created: HashMap<String, Identity>,
        updated: HashMap<String, Option<Identity>>,
        destroyed: Vec<String>,
        not_created: HashMap<String, SetError>,
        not_updated: HashMap<String, SetError>,
        not_destroyed: HashMap<String, SetError>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: SetJmapIdentitiesError,
    },
}

/// Arguments for an `Identity/set` request.
#[derive(Clone, Debug, Default)]
pub struct IdentitySetArgs {
    pub create: HashMap<String, IdentityCreate>,
    pub update: HashMap<String, IdentityUpdate>,
    pub destroy: Vec<String>,
}

impl IdentitySetArgs {
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
    not_created: Option<HashMap<String, SetError>>,
    #[serde(default)]
    not_updated: Option<HashMap<String, SetError>>,
    #[serde(default)]
    not_destroyed: Option<HashMap<String, SetError>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentitySetRequest {
    account_id: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    create: HashMap<String, IdentityCreate>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    update: HashMap<String, IdentityUpdate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    destroy: Vec<String>,
}

/// I/O-free coroutine for the JMAP `Identity/set` method.
pub struct SetJmapIdentities {
    send: SendJmapRequest,
}

impl SetJmapIdentities {
    pub fn new(
        context: JmapContext,
        args: IdentitySetArgs,
    ) -> Result<Self, SetJmapIdentitiesError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let json_args = serde_json::to_value(IdentitySetRequest {
            account_id,
            create: args.create,
            update: args.update,
            destroy: args.destroy,
        })
        .map_err(SetJmapIdentitiesError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add("Identity/set", json_args);
        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
            capabilities::SUBMISSION.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> SetJmapIdentitiesResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return SetJmapIdentitiesResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return SetJmapIdentitiesResult::Err { context, err: err.into() }
            }
        };

        let Some((name, args, _)) = response.method_responses.into_iter().next() else {
            return SetJmapIdentitiesResult::Err {
                context,
                err: SetJmapIdentitiesError::MissingResponse,
            };
        };

        if name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(args)
                .unwrap_or(JmapMethodError::Unknown);
            return SetJmapIdentitiesResult::Err { context, err: err.into() };
        }

        match serde_json::from_value::<IdentitySetResponse>(args) {
            Ok(r) => SetJmapIdentitiesResult::Ok {
                context,
                new_state: r.new_state.unwrap_or_default(),
                // created/updated are parsed as raw Value to tolerate partial
                // server responses; return empty maps since no caller uses them.
                created: HashMap::new(),
                updated: HashMap::new(),
                destroyed: r.destroyed.unwrap_or_default(),
                not_created: r.not_created.unwrap_or_default(),
                not_updated: r.not_updated.unwrap_or_default(),
                not_destroyed: r.not_destroyed.unwrap_or_default(),
                keep_alive,
            },
            Err(err) => SetJmapIdentitiesResult::Err {
                context,
                err: SetJmapIdentitiesError::ParseResponse(err),
            },
        }
    }
}
