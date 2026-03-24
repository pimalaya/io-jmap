//! I/O-free coroutine for batched `Mailbox/query` + `Mailbox/get` (RFC 8621 §2.4–2.5).
//!
//! Combines both methods into a **single HTTP request** using a JMAP
//! result reference so the server resolves the IDs without a second
//! round-trip.

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        error::JmapMethodError,
        mailbox::{Mailbox, MailboxFilter, MailboxProperty, MailboxSortComparator},
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum QueryJmapMailboxesError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Serialize Mailbox/query arguments error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse Mailbox/query response error: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    #[error("Parse Mailbox/get response error: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    #[error("Missing Mailbox/query response in method_responses")]
    MissingQueryResponse,
    #[error("Missing Mailbox/get response in method_responses")]
    MissingGetResponse,
    #[error("JMAP Mailbox/query method error: {0}")]
    QueryMethod(JmapMethodError),
    #[error("JMAP Mailbox/get method error: {0}")]
    GetMethod(JmapMethodError),
}

/// Result returned by the [`QueryJmapMailboxes`] coroutine.
#[derive(Debug)]
pub enum QueryJmapMailboxesResult {
    /// The coroutine successfully queried mailboxes.
    Ok {
        context: JmapContext,
        mailboxes: Vec<Mailbox>,
        total: Option<u64>,
        position: u64,
        query_state: String,
        keep_alive: bool,
    },
    /// The coroutine wants stream I/O.
    Io(StreamIo),
    /// The coroutine encountered an error.
    Err {
        context: JmapContext,
        err: QueryJmapMailboxesError,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a MailboxFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<&'a [MailboxSortComparator]>,
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
    ids_ref: ResultReference<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [MailboxProperty]>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultReference<'a> {
    result_of: &'a str,
    name: &'static str,
    path: &'static str,
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
    list: Vec<Mailbox>,
}

/// I/O-free coroutine for the combined `Mailbox/query` + `Mailbox/get` operation.
///
/// Sends a single batched JMAP request containing:
/// 1. `Mailbox/query` — finds mailbox IDs matching the filter
/// 2. `Mailbox/get` — fetches the specified properties for those IDs
///    using a JMAP Result Reference (back-reference from the query)
pub struct QueryJmapMailboxes {
    send: SendJmapRequest,
}

impl QueryJmapMailboxes {
    /// Creates a new coroutine.
    ///
    /// - `filter`: filter criteria, or `None` for all mailboxes
    /// - `sort`: sort order, or `None` for server default
    /// - `position`: zero-based offset into results
    /// - `limit`: maximum number of mailboxes to return
    /// - `properties`: mailbox properties to fetch, or `None` for all
    pub fn new(
        context: JmapContext,
        filter: Option<MailboxFilter>,
        sort: Option<Vec<MailboxSortComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
        properties: Option<Vec<MailboxProperty>>,
    ) -> Result<Self, QueryJmapMailboxesError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let query_args = MailboxQueryArgs {
            account_id: &account_id,
            filter: filter.as_ref(),
            sort: sort.as_deref(),
            position,
            limit,
            calculate_total: true,
        };

        let mut batch = JmapBatch::new();
        let query_id = batch.add(
            "Mailbox/query",
            serde_json::to_value(&query_args).map_err(QueryJmapMailboxesError::SerializeArgs)?,
        );

        let get_args = MailboxGetByRefArgs {
            account_id: &account_id,
            ids_ref: ResultReference {
                result_of: &query_id,
                name: "Mailbox/query",
                path: "/ids",
            },
            properties: properties.as_deref(),
        };

        batch.add(
            "Mailbox/get",
            serde_json::to_value(&get_args).map_err(QueryJmapMailboxesError::SerializeArgs)?,
        );

        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> QueryJmapMailboxesResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok {
                context,
                response,
                keep_alive,
            } => (context, response, keep_alive),
            SendJmapRequestResult::Io(io) => return QueryJmapMailboxesResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return QueryJmapMailboxesResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let mut responses = response.method_responses.into_iter();

        // Parse Mailbox/query response
        let Some((query_name, query_args, _)) = responses.next() else {
            return QueryJmapMailboxesResult::Err {
                context,
                err: QueryJmapMailboxesError::MissingQueryResponse,
            };
        };

        if query_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(query_args)
                .unwrap_or(JmapMethodError::Unknown);
            return QueryJmapMailboxesResult::Err {
                context,
                err: QueryJmapMailboxesError::QueryMethod(err),
            };
        }

        let query_response = match serde_json::from_value::<MailboxQueryResponse>(query_args) {
            Ok(r) => r,
            Err(err) => {
                return QueryJmapMailboxesResult::Err {
                    context,
                    err: QueryJmapMailboxesError::ParseQueryResponse(err),
                }
            }
        };

        // Parse Mailbox/get response
        let Some((get_name, get_args, _)) = responses.next() else {
            return QueryJmapMailboxesResult::Err {
                context,
                err: QueryJmapMailboxesError::MissingGetResponse,
            };
        };

        if get_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(get_args)
                .unwrap_or(JmapMethodError::Unknown);
            return QueryJmapMailboxesResult::Err {
                context,
                err: QueryJmapMailboxesError::GetMethod(err),
            };
        }

        match serde_json::from_value::<MailboxGetResponse>(get_args) {
            Ok(r) => QueryJmapMailboxesResult::Ok {
                context,
                mailboxes: r.list,
                total: query_response.total,
                position: query_response.position,
                query_state: query_response.query_state,
                keep_alive,
            },
            Err(err) => QueryJmapMailboxesResult::Err {
                context,
                err: QueryJmapMailboxesError::ParseGetResponse(err),
            },
        }
    }
}
