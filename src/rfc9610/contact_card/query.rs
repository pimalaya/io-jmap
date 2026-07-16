//! Batched JMAP `ContactCard/query` + `ContactCard/get` coroutine (RFC 9610
//! §3.1 and §3.3): single HTTP request, server-side `#ids` back-reference
//! resolves the get against the query results.
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
//!     rfc9610::contact_card::query::{JmapContactCardQuery, JmapContactCardQueryOptions},
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
//!     JmapContactCardQuery::new(&session, &auth, JmapContactCardQueryOptions::default())
//!         .unwrap();
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
//! println!("{} cards", out.cards.len());
//! ```

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY, JmapBatch, JmapMethodError, JmapResultReference, JmapSession, send::*,
    },
    rfc9610::{
        JMAP_CONTACTS_CAPABILITY,
        contact_card::{JmapContactCard, JmapContactCardFilter, JmapContactCardSortComparator},
    },
};

/// Failure causes during a batched JMAP `ContactCard/query` +
/// `ContactCard/get` flow.
#[derive(Debug, Error)]
pub enum JmapContactCardQueryError {
    /// The response carried no query response.
    #[error(
        "JMAP ContactCard/query failed: missing ContactCard/query response in method_responses"
    )]
    MissingQueryResponse,
    /// The response carried no get response.
    #[error("JMAP ContactCard/query failed: missing ContactCard/get response in method_responses")]
    MissingGetResponse,
    /// The inner send coroutine failed.
    #[error("JMAP ContactCard/query failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP ContactCard/query failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The query response could not be parsed.
    #[error("JMAP ContactCard/query failed: parse ContactCard/query response: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    /// The get response could not be parsed.
    #[error("JMAP ContactCard/query failed: parse ContactCard/get response: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    /// The server returned a method-level error for the query call.
    #[error("JMAP ContactCard/query failed: ContactCard/query: {0}")]
    QueryMethod(JmapMethodError),
    /// The server returned a method-level error for the get call.
    #[error("JMAP ContactCard/query failed: ContactCard/get: {0}")]
    GetMethod(JmapMethodError),
}

/// Options for [`JmapContactCardQuery::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapContactCardQueryOptions {
    /// Filter criteria; `None` matches all cards.
    pub filter: Option<JmapContactCardFilter>,
    /// Sort order; `None` uses the server default.
    pub sort: Option<Vec<JmapContactCardSortComparator>>,
    /// Zero-based offset into the result list.
    pub position: Option<u64>,
    /// Max number of cards to return.
    pub limit: Option<u64>,
    /// Card properties to fetch (JSContact property names plus `id` and
    /// `addressBookIds`); `None` returns all.
    pub properties: Option<Vec<String>>,
}

/// Successful terminal output of [`JmapContactCardQuery`].
#[derive(Clone, Debug)]
pub struct JmapContactCardQueryOutput {
    /// The fetched contact cards.
    pub cards: Vec<JmapContactCard>,
    /// The total number of matching objects, when the server computed it.
    pub total: Option<u64>,
    /// Zero-based index of the first returned id.
    pub position: u64,
    /// The state the query results were computed at.
    pub query_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the combined `ContactCard/query` +
/// `ContactCard/get` operation.
pub struct JmapContactCardQuery {
    state: State,
}

impl JmapContactCardQuery {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapContactCardQueryOptions,
    ) -> Result<Self, JmapContactCardQueryError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_CONTACTS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let query_args = ContactCardQueryArgs {
            account_id: &account_id,
            filter: opts.filter.as_ref(),
            sort: opts.sort.as_deref(),
            position: opts.position,
            limit: opts.limit,
            calculate_total: true,
        };

        let mut batch = JmapBatch::new();
        let query_id = batch.add(
            "ContactCard/query",
            serde_json::to_value(&query_args).map_err(JmapContactCardQueryError::SerializeArgs)?,
        );

        let get_args = ContactCardGetByRefArgs {
            account_id: &account_id,
            ids_ref: JmapResultReference {
                result_of: &query_id,
                name: "ContactCard/query",
                path: "/ids",
            },
            properties: opts.properties.as_deref(),
        };

        batch.add(
            "ContactCard/get",
            serde_json::to_value(&get_args).map_err(JmapContactCardQueryError::SerializeArgs)?,
        );

        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_CONTACTS_CAPABILITY.into(),
        ]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapContactCardQuery {
    type Yield = JmapYield;
    type Return = Result<JmapContactCardQueryOutput, JmapContactCardQueryError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let mut responses = response.method_responses.into_iter();

                let Some((query_name, query_args, _)) = responses.next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapContactCardQueryError::MissingQueryResponse,
                    ));
                };

                if query_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(query_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(
                        JmapContactCardQueryError::QueryMethod(err),
                    ));
                }

                let query_response =
                    match serde_json::from_value::<ContactCardQueryResponse>(query_args) {
                        Ok(r) => r,
                        Err(err) => {
                            return JmapCoroutineState::Complete(Err(
                                JmapContactCardQueryError::ParseQueryResponse(err),
                            ));
                        }
                    };

                let Some((get_name, get_args, _)) = responses.next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapContactCardQueryError::MissingGetResponse,
                    ));
                };

                if get_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(get_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(
                        JmapContactCardQueryError::GetMethod(err),
                    ));
                }

                match serde_json::from_value::<ContactCardGetResponse>(get_args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapContactCardQueryOutput {
                        cards: r.list,
                        total: query_response.total,
                        position: query_response.position,
                        query_state: query_response.query_state,
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapContactCardQueryError::ParseGetResponse(err),
                    )),
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
struct ContactCardQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a JmapContactCardFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<&'a [JmapContactCardSortComparator]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    calculate_total: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContactCardGetByRefArgs<'a> {
    account_id: &'a str,
    #[serde(rename = "#ids")]
    ids_ref: JmapResultReference<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [String]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContactCardQueryResponse {
    query_state: String,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    position: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContactCardGetResponse {
    list: Vec<JmapContactCard>,
}
