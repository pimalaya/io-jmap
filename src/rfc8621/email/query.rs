//! Batched JMAP `Email/query` + `Email/get` coroutine (RFC 8621 §4): a single
//! HTTP request that runs `Email/query` to find matching ids and `Email/get`
//! (via a Result Reference) to fetch their properties.
//!
//! Equivalent to IMAP's `SELECT` + `SEARCH` + `FETCH` but in one round trip;
//! the result reference (`#ids`) keeps the two method calls linked server-side.
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
//!     rfc8621::email::query::{JmapEmailQuery, JmapEmailQueryOptions},
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
//! let mut coroutine = JmapEmailQuery::new(
//!     &session,
//!     &auth,
//!     JmapEmailQueryOptions {
//!         position: Some(0),
//!         limit: Some(20),
//!         ..Default::default()
//!     },
//! )
//! .unwrap();
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
//! println!("{} emails", out.emails.len());
//! ```

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY, JmapBatch, JmapFilter, JmapMethodError, JmapResultReference,
        JmapSession, send::*,
    },
    rfc8621::{
        JMAP_MAIL_CAPABILITY,
        email::{JmapEmail, JmapEmailComparator, JmapEmailFilter, JmapEmailProperty},
    },
};

/// Failure causes during a batched JMAP `Email/query` + `Email/get` flow.
#[derive(Debug, Error)]
pub enum JmapEmailQueryError {
    /// The response carried no query response.
    #[error("JMAP Email/query failed: missing Email/query response in method_responses")]
    MissingQueryResponse,
    /// The response carried no get response.
    #[error("JMAP Email/query failed: missing Email/get response in method_responses")]
    MissingGetResponse,
    /// The inner send coroutine failed.
    #[error("JMAP Email/query failed: {0}")]
    Send(#[from] JmapSendError),
    /// The query response could not be parsed.
    #[error("JMAP Email/query failed: parse Email/query response: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    /// The get response could not be parsed.
    #[error("JMAP Email/query failed: parse Email/get response: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    /// The server returned a method-level error for the query call.
    #[error("JMAP Email/query failed: Email/query: {0}")]
    QueryMethod(JmapMethodError),
    /// The server returned a method-level error for the get call.
    #[error("JMAP Email/query failed: Email/get: {0}")]
    GetMethod(JmapMethodError),
}

/// Options for [`JmapEmailQuery::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapEmailQueryOptions {
    /// Filter criteria; `None` matches all emails.
    pub filter: Option<JmapFilter<JmapEmailFilter>>,
    /// Sort order; `None` uses the server default.
    pub sort: Option<Vec<JmapEmailComparator>>,
    /// Zero-based offset into the result list.
    pub position: Option<u64>,
    /// Max number of emails to return.
    pub limit: Option<u64>,
    /// Email properties to fetch; `None` returns all.
    pub properties: Option<Vec<JmapEmailProperty>>,
}

/// Successful terminal output of [`JmapEmailQuery`].
#[derive(Clone, Debug)]
pub struct JmapEmailQueryOutput {
    /// The fetched emails.
    pub emails: Vec<JmapEmail>,
    /// The total number of matching objects, when the server computed it.
    pub total: Option<u64>,
    /// Zero-based index of the first returned id.
    pub position: u64,
    /// The state the query results were computed at.
    pub query_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the combined `Email/query` + `Email/get` operation.
pub struct JmapEmailQuery {
    state: State,
}

impl JmapEmailQuery {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapEmailQueryOptions,
    ) -> Result<Self, JmapEmailQueryError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let query_args = EmailQueryArgs {
            account_id: &account_id,
            filter: opts.filter.as_ref(),
            sort: opts.sort.as_deref(),
            position: opts.position,
            limit: opts.limit,
            calculate_total: true,
        };

        let mut batch = JmapBatch::new();
        let query_id = batch.add(
            "Email/query",
            serde_json::to_value(&query_args).map_err(JmapEmailQueryError::ParseQueryResponse)?,
        );

        let get_args = EmailGetByRefArgs {
            account_id: &account_id,
            ids_ref: JmapResultReference {
                result_of: &query_id,
                name: "Email/query",
                path: "/ids",
            },
            properties: opts.properties.as_deref(),
        };

        batch.add(
            "Email/get",
            serde_json::to_value(&get_args).map_err(JmapEmailQueryError::ParseQueryResponse)?,
        );

        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_MAIL_CAPABILITY.into(),
        ]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapEmailQuery {
    type Yield = JmapYield;
    type Return = Result<JmapEmailQueryOutput, JmapEmailQueryError>;

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
                        JmapEmailQueryError::MissingQueryResponse,
                    ));
                };

                if query_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(query_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(JmapEmailQueryError::QueryMethod(
                        err,
                    )));
                }

                let query_response = match serde_json::from_value::<EmailQueryResponse>(query_args)
                {
                    Ok(r) => r,
                    Err(err) => {
                        return JmapCoroutineState::Complete(Err(
                            JmapEmailQueryError::ParseQueryResponse(err),
                        ));
                    }
                };

                let Some((get_name, get_args, _)) = responses.next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapEmailQueryError::MissingGetResponse,
                    ));
                };

                if get_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(get_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(JmapEmailQueryError::GetMethod(err)));
                }

                match serde_json::from_value::<EmailGetResponse>(get_args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailQueryOutput {
                        emails: r.list,
                        total: query_response.total,
                        position: query_response.position,
                        query_state: query_response.query_state,
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapEmailQueryError::ParseGetResponse(err),
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
struct EmailQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a JmapFilter<JmapEmailFilter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<&'a [JmapEmailComparator]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    calculate_total: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailGetByRefArgs<'a> {
    account_id: &'a str,
    #[serde(rename = "#ids")]
    ids_ref: JmapResultReference<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [JmapEmailProperty]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailQueryResponse {
    query_state: String,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    position: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmailGetResponse {
    list: Vec<JmapEmail>,
}
