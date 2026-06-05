//! Batched JMAP `EmailSubmission/query` + `EmailSubmission/get`
//! coroutine (RFC 8621 §7.3 + §7.2): one HTTP request, server-side
//! `#ids` back-reference.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::email_submission::query::JmapEmailSubmissionQuery,
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine =
//!     JmapEmailSubmissionQuery::new(session, &auth, None, None, None, None).unwrap();
//! # let _ = coroutine;
//! # }
//! ```

use core::fmt;

use alloc::{string::String, vec, vec::Vec};

use log::trace;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::coroutine::*;
use crate::jmap_try;
use crate::{
    rfc8620::{CORE_CAPABILITY, JmapBatch, JmapMethodError, JmapSession, ResultReference, send::*},
    rfc8621::{
        MAIL_CAPABILITY,
        email_submission::{
            EmailSubmission, EmailSubmissionComparator, EmailSubmissionFilter,
            SUBMISSION_CAPABILITY,
        },
    },
};

/// Failure causes during a batched JMAP `EmailSubmission/query` + `/get` flow.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionQueryError {
    #[error(
        "JMAP EmailSubmission/query failed: missing EmailSubmission/query response in method_responses"
    )]
    MissingQueryResponse,
    #[error(
        "JMAP EmailSubmission/query failed: missing EmailSubmission/get response in method_responses"
    )]
    MissingGetResponse,
    #[error("JMAP EmailSubmission/query failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP EmailSubmission/query failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    #[error("JMAP EmailSubmission/query failed: parse EmailSubmission/query response: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    #[error("JMAP EmailSubmission/query failed: parse EmailSubmission/get response: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    #[error("JMAP EmailSubmission/query failed: EmailSubmission/query: {0}")]
    QueryMethod(JmapMethodError),
    #[error("JMAP EmailSubmission/query failed: EmailSubmission/get: {0}")]
    GetMethod(JmapMethodError),
}

/// Successful terminal output of [`JmapEmailSubmissionQuery`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionQueryOutput {
    pub submissions: Vec<EmailSubmission>,
    pub total: Option<u64>,
    pub position: u64,
    pub query_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for batched `EmailSubmission/query` +
/// `EmailSubmission/get`.
pub struct JmapEmailSubmissionQuery {
    state: State,
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
            .get(MAIL_CAPABILITY)
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
            CORE_CAPABILITY.into(),
            MAIL_CAPABILITY.into(),
            SUBMISSION_CAPABILITY.into(),
        ]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapEmailSubmissionQuery {
    type Yield = JmapYield;
    type Return = Result<JmapEmailSubmissionQueryOutput, JmapEmailSubmissionQueryError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("EmailSubmission/query: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let mut responses = response.method_responses.into_iter();

                let Some((query_name, query_args, _)) = responses.next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapEmailSubmissionQueryError::MissingQueryResponse,
                    ));
                };

                if query_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(query_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(
                        JmapEmailSubmissionQueryError::QueryMethod(err),
                    ));
                }

                let query_response =
                    match serde_json::from_value::<SubmissionQueryResponse>(query_args) {
                        Ok(r) => r,
                        Err(err) => {
                            return JmapCoroutineState::Complete(Err(
                                JmapEmailSubmissionQueryError::ParseQueryResponse(err),
                            ));
                        }
                    };

                let Some((get_name, get_args, _)) = responses.next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapEmailSubmissionQueryError::MissingGetResponse,
                    ));
                };

                if get_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(get_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(
                        JmapEmailSubmissionQueryError::GetMethod(err),
                    ));
                }

                match serde_json::from_value::<SubmissionGetResponse>(get_args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapEmailSubmissionQueryOutput {
                        submissions: r.list,
                        total: query_response.total,
                        position: query_response.position,
                        query_state: query_response.query_state,
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapEmailSubmissionQueryError::ParseGetResponse(err),
                    )),
                }
            }
        }
    }
}

enum State {
    Send(JmapSend),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
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
