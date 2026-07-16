//! Batched JMAP `Mailbox/query` + `Mailbox/get` coroutine (RFC 8621 §2.4–2.5):
//! single HTTP request, server-side `#ids` back-reference resolves the get
//! against the query results.
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
//!     rfc8621::mailbox::query::{JmapMailboxQuery, JmapMailboxQueryOptions},
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
//!     JmapMailboxQuery::new(&session, &auth, JmapMailboxQueryOptions::default()).unwrap();
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
//! println!("{} mailboxes", out.mailboxes.len());
//! ```

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY, error::JmapMethodError, request::JmapBatch,
        request::JmapResultReference, send::*, session::JmapSession,
    },
    rfc8621::{
        JMAP_MAIL_CAPABILITY,
        mailbox::{JmapMailbox, JmapMailboxProperty, JmapMailboxRole},
    },
};

/// Sort property for `Mailbox/query` (RFC 8621 §2.4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JmapMailboxSortProperty {
    /// Sort by mailbox name.
    Name,
    /// Sort by the sortOrder position hint.
    SortOrder,
    /// Sort by parent mailbox id.
    ParentId,
}

/// Sort comparator for `Mailbox/query` (RFC 8620 §5.5).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxSortComparator {
    /// The property to sort by.
    pub property: JmapMailboxSortProperty,
    /// Ascending if `None` or `Some(true)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,
}

/// Filter condition for `Mailbox/query` (RFC 8621 §2.4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapMailboxFilter {
    /// Filter by parent mailbox ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Filter by role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<JmapMailboxRole>,
    /// Filter by name (substring match).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether to include subscribed mailboxes only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
    /// Whether to include mailboxes with a role only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_any_role: Option<bool>,
}

/// Failure causes during a batched JMAP `Mailbox/query` + `Mailbox/get` flow.
#[derive(Debug, Error)]
pub enum JmapMailboxQueryError {
    /// The response carried no query response.
    #[error("JMAP Mailbox/query failed: missing Mailbox/query response in method_responses")]
    MissingQueryResponse,
    /// The response carried no get response.
    #[error("JMAP Mailbox/query failed: missing Mailbox/get response in method_responses")]
    MissingGetResponse,
    /// The inner send coroutine failed.
    #[error("JMAP Mailbox/query failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP Mailbox/query failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The query response could not be parsed.
    #[error("JMAP Mailbox/query failed: parse Mailbox/query response: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    /// The get response could not be parsed.
    #[error("JMAP Mailbox/query failed: parse Mailbox/get response: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    /// The server returned a method-level error for the query call.
    #[error("JMAP Mailbox/query failed: Mailbox/query: {0}")]
    QueryMethod(JmapMethodError),
    /// The server returned a method-level error for the get call.
    #[error("JMAP Mailbox/query failed: Mailbox/get: {0}")]
    GetMethod(JmapMethodError),
}

/// Options for [`JmapMailboxQuery::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapMailboxQueryOptions {
    /// Filter criteria; `None` matches all mailboxes.
    pub filter: Option<JmapMailboxFilter>,
    /// Sort order; `None` uses the server default.
    pub sort: Option<Vec<JmapMailboxSortComparator>>,
    /// Zero-based offset into the result list.
    pub position: Option<u64>,
    /// Max number of mailboxes to return.
    pub limit: Option<u64>,
    /// Mailbox properties to fetch; `None` returns all.
    pub properties: Option<Vec<JmapMailboxProperty>>,
}

/// Successful terminal output of [`JmapMailboxQuery`].
#[derive(Clone, Debug)]
pub struct JmapMailboxQueryOutput {
    /// The fetched mailboxes.
    pub mailboxes: Vec<JmapMailbox>,
    /// The total number of matching objects, when the server computed it.
    pub total: Option<u64>,
    /// Zero-based index of the first returned id.
    pub position: u64,
    /// The state the query results were computed at.
    pub query_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the combined `Mailbox/query` + `Mailbox/get`
/// operation.
pub struct JmapMailboxQuery {
    state: State,
}

impl JmapMailboxQuery {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapMailboxQueryOptions,
    ) -> Result<Self, JmapMailboxQueryError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let query_args = MailboxQueryArgs {
            account_id: &account_id,
            filter: opts.filter.as_ref(),
            sort: opts.sort.as_deref(),
            position: opts.position,
            limit: opts.limit,
            calculate_total: true,
        };

        let mut batch = JmapBatch::new();
        let query_id = batch.add(
            "Mailbox/query",
            serde_json::to_value(&query_args).map_err(JmapMailboxQueryError::SerializeArgs)?,
        );

        let get_args = MailboxGetByRefArgs {
            account_id: &account_id,
            ids_ref: JmapResultReference {
                result_of: &query_id,
                name: "Mailbox/query",
                path: "/ids",
            },
            properties: opts.properties.as_deref(),
        };

        batch.add(
            "Mailbox/get",
            serde_json::to_value(&get_args).map_err(JmapMailboxQueryError::SerializeArgs)?,
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

impl JmapCoroutine for JmapMailboxQuery {
    type Yield = JmapYield;
    type Return = Result<JmapMailboxQueryOutput, JmapMailboxQueryError>;

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
                        JmapMailboxQueryError::MissingQueryResponse,
                    ));
                };

                if query_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(query_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(JmapMailboxQueryError::QueryMethod(
                        err,
                    )));
                }

                let query_response =
                    match serde_json::from_value::<MailboxQueryResponse>(query_args) {
                        Ok(r) => r,
                        Err(err) => {
                            return JmapCoroutineState::Complete(Err(
                                JmapMailboxQueryError::ParseQueryResponse(err),
                            ));
                        }
                    };

                let Some((get_name, get_args, _)) = responses.next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapMailboxQueryError::MissingGetResponse,
                    ));
                };

                if get_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(get_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(JmapMailboxQueryError::GetMethod(
                        err,
                    )));
                }

                match serde_json::from_value::<MailboxGetResponse>(get_args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapMailboxQueryOutput {
                        mailboxes: r.list,
                        total: query_response.total,
                        position: query_response.position,
                        query_state: query_response.query_state,
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapMailboxQueryError::ParseGetResponse(err),
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
struct MailboxQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a JmapMailboxFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<&'a [JmapMailboxSortComparator]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    calculate_total: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxGetByRefArgs<'a> {
    account_id: &'a str,
    #[serde(rename = "#ids")]
    ids_ref: JmapResultReference<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [JmapMailboxProperty]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MailboxQueryResponse {
    query_state: String,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    position: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MailboxGetResponse {
    list: Vec<JmapMailbox>,
}
