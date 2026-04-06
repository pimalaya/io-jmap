//! I/O-free coroutine for batched `EmailSubmission/query` + `EmailSubmission/get`
//! (RFC 8621 §7.3–7.2).

use alloc::{string::String, vec, vec::Vec};
use io_socket::io::{SocketInput, SocketOutput};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    rfc8620::error::JmapMethodError,
    rfc8620::result_reference::ResultReference,
    rfc8620::send::{JmapBatch, JmapSend, JmapSendError, JmapSendResult},
    rfc8620::session::JmapSession,
    rfc8621::capabilities,
    rfc8621::email_submission::{
        EmailSubmission, EmailSubmissionComparator, EmailSubmissionFilter,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionQueryError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] JmapSendError),
    #[error("Serialize EmailSubmission/query args error: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("Parse EmailSubmission/query response error: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    #[error("Parse EmailSubmission/get response error: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    #[error("Missing EmailSubmission/query response in method_responses")]
    MissingQueryResponse,
    #[error("Missing EmailSubmission/get response in method_responses")]
    MissingGetResponse,
    #[error("JMAP EmailSubmission/query method error: {0}")]
    QueryMethod(JmapMethodError),
    #[error("JMAP EmailSubmission/get method error: {0}")]
    GetMethod(JmapMethodError),
}

/// Result returned by the [`JmapEmailSubmissionQuery`] coroutine.
#[derive(Debug)]
pub enum JmapEmailSubmissionQueryResult {
    Ok {
        submissions: Vec<EmailSubmission>,
        total: Option<u64>,
        position: u64,
        query_state: String,
        keep_alive: bool,
    },
    Io {
        input: SocketInput,
    },
    Err {
        err: JmapEmailSubmissionQueryError,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmissionQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a EmailSubmissionFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<&'a [EmailSubmissionComparator]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    calculate_total: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmissionGetByRefArgs<'a> {
    account_id: &'a str,
    #[serde(rename = "#ids")]
    ids_ref: ResultReference<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmissionQueryResponse {
    query_state: String,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    position: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmissionGetResponse {
    list: Vec<EmailSubmission>,
}

/// I/O-free coroutine for batched `EmailSubmission/query` + `EmailSubmission/get`.
pub struct JmapEmailSubmissionQuery {
    send: JmapSend,
}

impl JmapEmailSubmissionQuery {
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        filter: Option<EmailSubmissionFilter>,
        sort: Option<Vec<EmailSubmissionComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Self, JmapEmailSubmissionQueryError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let query_args = SubmissionQueryArgs {
            account_id: &account_id,
            filter: filter.as_ref(),
            sort: sort.as_deref(),
            position,
            limit,
            calculate_total: true,
        };

        let mut batch = JmapBatch::new();
        let query_id = batch.add(
            "EmailSubmission/query",
            serde_json::to_value(&query_args)
                .map_err(JmapEmailSubmissionQueryError::SerializeArgs)?,
        );

        let get_args = SubmissionGetByRefArgs {
            account_id: &account_id,
            ids_ref: ResultReference {
                result_of: &query_id,
                name: "EmailSubmission/query",
                path: "/ids",
            },
        };

        batch.add(
            "EmailSubmission/get",
            serde_json::to_value(&get_args)
                .map_err(JmapEmailSubmissionQueryError::SerializeArgs)?,
        );

        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
            capabilities::SUBMISSION.into(),
        ]);

        Ok(Self {
            send: JmapSend::new(http_auth, api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapEmailSubmissionQueryResult {
        let (response, keep_alive) = match self.send.resume(arg) {
            JmapSendResult::Ok {
                response,
                keep_alive,
            } => (response, keep_alive),
            JmapSendResult::Io { input } => return JmapEmailSubmissionQueryResult::Io { input },
            JmapSendResult::Err { err } => {
                return JmapEmailSubmissionQueryResult::Err { err: err.into() };
            }
        };

        let mut responses = response.method_responses.into_iter();

        let Some((query_name, query_args, _)) = responses.next() else {
            return JmapEmailSubmissionQueryResult::Err {
                err: JmapEmailSubmissionQueryError::MissingQueryResponse,
            };
        };

        if query_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(query_args)
                .unwrap_or(JmapMethodError::Unknown);
            return JmapEmailSubmissionQueryResult::Err {
                err: JmapEmailSubmissionQueryError::QueryMethod(err),
            };
        }

        let query_response = match serde_json::from_value::<SubmissionQueryResponse>(query_args) {
            Ok(r) => r,
            Err(err) => {
                return JmapEmailSubmissionQueryResult::Err {
                    err: JmapEmailSubmissionQueryError::ParseQueryResponse(err),
                };
            }
        };

        let Some((get_name, get_args, _)) = responses.next() else {
            return JmapEmailSubmissionQueryResult::Err {
                err: JmapEmailSubmissionQueryError::MissingGetResponse,
            };
        };

        if get_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(get_args)
                .unwrap_or(JmapMethodError::Unknown);
            return JmapEmailSubmissionQueryResult::Err {
                err: JmapEmailSubmissionQueryError::GetMethod(err),
            };
        }

        match serde_json::from_value::<SubmissionGetResponse>(get_args) {
            Ok(r) => JmapEmailSubmissionQueryResult::Ok {
                submissions: r.list,
                total: query_response.total,
                position: query_response.position,
                query_state: query_response.query_state,
                keep_alive,
            },
            Err(err) => JmapEmailSubmissionQueryResult::Err {
                err: JmapEmailSubmissionQueryError::ParseGetResponse(err),
            },
        }
    }
}
