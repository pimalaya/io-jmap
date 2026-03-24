//! I/O-free coroutine for batched `EmailSubmission/query` + `EmailSubmission/get`
//! (RFC 8621 §7.3–7.2).

use io_stream::io::StreamIo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    context::JmapContext,
    coroutines::send::{JmapBatch, SendJmapRequest, SendJmapRequestError, SendJmapRequestResult},
    types::{
        email_submission::{EmailSubmission, EmailSubmissionComparator, EmailSubmissionFilter},
        error::JmapMethodError,
        session::capabilities,
    },
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum QueryJmapEmailSubmissionsError {
    #[error("Send JMAP request error: {0}")]
    Send(#[from] SendJmapRequestError),
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

/// Result returned by the [`QueryJmapEmailSubmissions`] coroutine.
#[derive(Debug)]
pub enum QueryJmapEmailSubmissionsResult {
    Ok {
        context: JmapContext,
        submissions: Vec<EmailSubmission>,
        total: Option<u64>,
        position: u64,
        query_state: String,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: QueryJmapEmailSubmissionsError,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultReference<'a> {
    result_of: &'a str,
    name: &'static str,
    path: &'static str,
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
pub struct QueryJmapEmailSubmissions {
    send: SendJmapRequest,
}

impl QueryJmapEmailSubmissions {
    pub fn new(
        context: JmapContext,
        filter: Option<EmailSubmissionFilter>,
        sort: Option<Vec<EmailSubmissionComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Self, QueryJmapEmailSubmissionsError> {
        let account_id = context.account_id.clone().unwrap_or_default();
        let api_url = context
            .api_url()
            .cloned()
            .unwrap_or_else(|| "http://localhost".parse().unwrap());

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
                .map_err(QueryJmapEmailSubmissionsError::SerializeArgs)?,
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
                .map_err(QueryJmapEmailSubmissionsError::SerializeArgs)?,
        );

        let request = batch.into_request(vec![
            capabilities::CORE.into(),
            capabilities::MAIL.into(),
            capabilities::SUBMISSION.into(),
        ]);

        Ok(Self {
            send: SendJmapRequest::new(context, &api_url, request)?,
        })
    }

    pub fn resume(&mut self, arg: Option<StreamIo>) -> QueryJmapEmailSubmissionsResult {
        let (context, response, keep_alive) = match self.send.resume(arg) {
            SendJmapRequestResult::Ok { context, response, keep_alive } => {
                (context, response, keep_alive)
            }
            SendJmapRequestResult::Io(io) => return QueryJmapEmailSubmissionsResult::Io(io),
            SendJmapRequestResult::Err { context, err } => {
                return QueryJmapEmailSubmissionsResult::Err { context, err: err.into() }
            }
        };

        let mut responses = response.method_responses.into_iter();

        let Some((query_name, query_args, _)) = responses.next() else {
            return QueryJmapEmailSubmissionsResult::Err {
                context,
                err: QueryJmapEmailSubmissionsError::MissingQueryResponse,
            };
        };

        if query_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(query_args)
                .unwrap_or(JmapMethodError::Unknown);
            return QueryJmapEmailSubmissionsResult::Err {
                context,
                err: QueryJmapEmailSubmissionsError::QueryMethod(err),
            };
        }

        let query_response =
            match serde_json::from_value::<SubmissionQueryResponse>(query_args) {
                Ok(r) => r,
                Err(err) => {
                    return QueryJmapEmailSubmissionsResult::Err {
                        context,
                        err: QueryJmapEmailSubmissionsError::ParseQueryResponse(err),
                    }
                }
            };

        let Some((get_name, get_args, _)) = responses.next() else {
            return QueryJmapEmailSubmissionsResult::Err {
                context,
                err: QueryJmapEmailSubmissionsError::MissingGetResponse,
            };
        };

        if get_name == "error" {
            let err = serde_json::from_value::<JmapMethodError>(get_args)
                .unwrap_or(JmapMethodError::Unknown);
            return QueryJmapEmailSubmissionsResult::Err {
                context,
                err: QueryJmapEmailSubmissionsError::GetMethod(err),
            };
        }

        match serde_json::from_value::<SubmissionGetResponse>(get_args) {
            Ok(r) => QueryJmapEmailSubmissionsResult::Ok {
                context,
                submissions: r.list,
                total: query_response.total,
                position: query_response.position,
                query_state: query_response.query_state,
                keep_alive,
            },
            Err(err) => QueryJmapEmailSubmissionsResult::Err {
                context,
                err: QueryJmapEmailSubmissionsError::ParseGetResponse(err),
            },
        }
    }
}
