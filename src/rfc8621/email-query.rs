//! I/O-free coroutine for batched `Email/query` + `Email/get` (RFC 8621 §4).
//!
//! This coroutine combines `Email/query` (to find matching email IDs)
//! and `Email/get` (to fetch their properties) into a **single HTTP
//! request** using JMAP's batching and result reference features.
//! This is a key performance advantage over IMAP, which requires
//! multiple round-trips for the same operation.

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    rfc8620::error::JmapMethodError,
    rfc8620::result_reference::ResultReference,
    rfc8620::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
    rfc8621::email::{Email, EmailComparator, EmailFilter, EmailProperty},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailQueryError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
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

/// Result returned by the [`JmapEmailQuery`] coroutine.
#[derive(Debug)]
pub enum JmapEmailQueryResult {
    /// The coroutine has successfully completed.
    Ok {
        emails: Vec<Email>,
        total: Option<u64>,
        position: u64,
        query_state: String,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(JmapEmailQueryError),
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
pub struct JmapEmailQuery {
    send: JmapSend,
}

impl JmapEmailQuery {
    /// Creates a new coroutine.
    ///
    /// - `filter`: filter criteria (pass `None` for all emails)
    /// - `sort`: sort order (pass `None` for default)
    /// - `position`: offset from the start of the results
    /// - `limit`: maximum number of emails to return
    /// - `properties`: email properties to fetch (pass `None` for all)
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        filter: Option<EmailFilter>,
        sort: Option<Vec<EmailComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<Self, JmapEmailQueryError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

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
            serde_json::to_value(&query_args).map_err(JmapEmailQueryError::ParseQueryResponse)?,
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
            serde_json::to_value(&get_args).map_err(JmapEmailQueryError::ParseQueryResponse)?,
        );

        let request =
            batch.into_request(vec![capabilities::CORE.into(), capabilities::MAIL.into()]);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapEmailQueryResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::WantsRead => return JmapEmailQueryResult::WantsRead,
            JmapSendResult::WantsWrite(bytes) => return JmapEmailQueryResult::WantsWrite(bytes),
            JmapSendResult::Err(err) => return JmapEmailQueryResult::Err(err.into()),
        };

        let mut responses = response.method_responses.into_iter();

        let Some((query_name, query_args, _)) = responses.next() else {
            return JmapEmailQueryResult::Err(JmapEmailQueryError::MissingQueryResponse);
        };

        if query_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(query_args)
                .unwrap_or(JmapMethodError::Unknown);
            return JmapEmailQueryResult::Err(JmapEmailQueryError::QueryMethod(err));
        }

        let query_response = match serde_json::from_value::<EmailQueryResponse>(query_args) {
            Ok(r) => r,
            Err(err) => {
                return JmapEmailQueryResult::Err(JmapEmailQueryError::ParseQueryResponse(err));
            }
        };

        let Some((get_name, get_args, _)) = responses.next() else {
            return JmapEmailQueryResult::Err(JmapEmailQueryError::MissingGetResponse);
        };

        if get_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(get_args)
                .unwrap_or(JmapMethodError::Unknown);
            return JmapEmailQueryResult::Err(JmapEmailQueryError::GetMethod(err));
        }

        match serde_json::from_value::<EmailGetResponse>(get_args) {
            Ok(r) => JmapEmailQueryResult::Ok {
                emails: r.list,
                total: query_response.total,
                position: query_response.position,
                query_state: query_response.query_state,
                keep_alive,
            },
            Err(err) => JmapEmailQueryResult::Err(JmapEmailQueryError::ParseGetResponse(err)),
        }
    }
}
