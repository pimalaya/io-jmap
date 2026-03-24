//! I/O-free coroutine for batched `Email/query` + `Email/get` (RFC 8621 §4).
//!
//! This coroutine combines `Email/query` (to find matching email IDs)
//! and `Email/get` (to fetch their properties) into a **single HTTP
//! request** using JMAP's batching and result reference features.
//! This is a key performance advantage over IMAP, which requires
//! multiple round-trips for the same operation.

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        email::{Email, EmailComparator, EmailFilter, EmailProperty},
        error::JmapMethodError,
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum QueryJmapEmailsError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
    #[error("Parse Email/query response error: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    #[error("Parse Email/get response error: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    #[error("Missing Email/query response in method_responses")]
    MissingQueryResponse,
    #[error("Missing Email/get response in method_responses")]
    MissingGetResponse,
    #[error("JMAP Email/query method error: {0}")]
    QueryMethod(JmapMethodError),
    #[error("JMAP Email/get method error: {0}")]
    GetMethod(JmapMethodError),
}

/// Result returned by the [`QueryJmapEmails`] coroutine.
#[derive(Debug)]
pub enum QueryJmapEmailsResult {
    Ok {
        context: JmapContext,
        emails: Vec<Email>,
        total: Option<u64>,
        position: u64,
        query_state: String,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: QueryJmapEmailsError,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a EmailFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<&'a [EmailComparator]>,
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
    ids_ref: ResultReference<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [EmailProperty]>,
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
    list: Vec<Email>,
}

/// I/O-free coroutine for the combined `Email/query` + `Email/get` operation.
///
/// This coroutine sends a single batched JMAP request containing:
/// 1. `Email/query` — finds email IDs matching the filter
/// 2. `Email/get` — fetches the specified properties for those IDs
///    using a JMAP Result Reference (back-reference from the query)
///
/// This is equivalent to IMAP's `SELECT` + `SEARCH` + `FETCH` but
/// in a single HTTP round-trip.
pub struct QueryJmapEmails {
    send: SendJmapRequest,
}

impl QueryJmapEmails {
    /// Creates a new coroutine.
    ///
    /// - `filter`: filter criteria (pass `None` for all emails)
    /// - `sort`: sort order (pass `None` for default)
    /// - `position`: offset from the start of the results
    /// - `limit`: maximum number of emails to return
    /// - `properties`: email properties to fetch (pass `None` for all)
    pub fn new(
        context: JmapContext,
        filter: Option<EmailFilter>,
        sort: Option<Vec<EmailComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<Self, QueryJmapEmailsError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

        let query_args = EmailQueryArgs {
            account_id: &account_id,
            filter: filter.as_ref(),
            sort: sort.as_deref(),
            position,
            limit,
            calculate_total: true,
        };

        let mut batch = JmapBatch::new();
        let query_id = batch.add(
            "Email/query",
            serde_json::to_value(&query_args).map_err(QueryJmapEmailsError::ParseQueryResponse)?,
        );

        let get_args = EmailGetByRefArgs {
            account_id: &account_id,
            ids_ref: ResultReference {
                result_of: &query_id,
                name: "Email/query",
                path: "/ids",
            },
            properties: properties.as_deref(),
        };

        batch.add(
            "Email/get",
            serde_json::to_value(&get_args).map_err(QueryJmapEmailsError::ParseQueryResponse)?,
        );

        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> QueryJmapEmailsResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok {
                context,
                response,
                keep_alive,
            } => (context, response, keep_alive),
            SendJmapRequestResult::Io(io) => return QueryJmapEmailsResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return QueryJmapEmailsResult::Err {
                    context,
                    err: err.into(),
                }
            }
        };

        let mut responses = response.method_responses.into_iter();

        // Parse Email/query response
        let Some((query_name, query_args, _)) = responses.next() else {
            return QueryJmapEmailsResult::Err {
                context,
                err: QueryJmapEmailsError::MissingQueryResponse,
            };
        };

        if query_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(query_args)
                .unwrap_or(JmapMethodError::Unknown);
            return QueryJmapEmailsResult::Err {
                context,
                err: QueryJmapEmailsError::QueryMethod(err),
            };
        }

        let query_response = match serde_json::from_value::<EmailQueryResponse>(query_args) {
            Ok(r) => r,
            Err(err) => {
                return QueryJmapEmailsResult::Err {
                    context,
                    err: QueryJmapEmailsError::ParseQueryResponse(err),
                }
            }
        };

        // Parse Email/get response
        let Some((get_name, get_args, _)) = responses.next() else {
            return QueryJmapEmailsResult::Err {
                context,
                err: QueryJmapEmailsError::MissingGetResponse,
            };
        };

        if get_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(get_args)
                .unwrap_or(JmapMethodError::Unknown);
            return QueryJmapEmailsResult::Err {
                context,
                err: QueryJmapEmailsError::GetMethod(err),
            };
        }

        match serde_json::from_value::<EmailGetResponse>(get_args) {
            Ok(r) => QueryJmapEmailsResult::Ok {
                context,
                emails: r.list,
                total: query_response.total,
                position: query_response.position,
                query_state: query_response.query_state,
                keep_alive,
            },
            Err(err) => QueryJmapEmailsResult::Err {
                context,
                err: QueryJmapEmailsError::ParseGetResponse(err),
            },
        }
    }
}
