//! Batched JMAP `EmailSubmission/query` + `EmailSubmission/get` coroutine (RFC
//! 8621 §7.3 + §7.2): one HTTP request, server-side `#ids` back-reference.
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
//!     rfc8621::email_submission::query::{
//!         JmapEmailSubmissionQuery, JmapEmailSubmissionQueryOptions,
//!     },
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
//! let mut coroutine = JmapEmailSubmissionQuery::new(
//!     &session,
//!     &auth,
//!     JmapEmailSubmissionQueryOptions::default(),
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
//! println!("{} submissions", out.submissions.len());
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
        email_submission::{JMAP_SUBMISSION_CAPABILITY, JmapEmailSubmission, JmapUndoStatus},
    },
};

/// Filter condition for `EmailSubmission/query` (RFC 8621 §7.4).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailSubmissionFilter {
    /// Only submissions sent from these identities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_ids: Option<Vec<String>>,
    /// Only submissions of these emails.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_ids: Option<Vec<String>>,
    /// Only submissions of emails in these threads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ids: Option<Vec<String>>,
    /// Only submissions with this undo status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo_status: Option<JmapUndoStatus>,
    /// RFC 3339 upper bound on the sendAt date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// RFC 3339 lower bound on the sendAt date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// Sort property for `EmailSubmission/query`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JmapEmailSubmissionSortProperty {
    /// Sort by email id.
    EmailId,
    /// Sort by thread id.
    ThreadId,
    /// Sort by the sendAt date.
    SentAt,
}

/// Sort comparator for `EmailSubmission/query`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailSubmissionComparator {
    /// The property to sort by.
    pub property: JmapEmailSubmissionSortProperty,
    /// Ascending if `None` or `Some(true)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,
}

/// Failure causes during a batched JMAP `EmailSubmission/query` + `/get` flow.
#[derive(Debug, Error)]
pub enum JmapEmailSubmissionQueryError {
    /// The response carried no query response.
    #[error(
        "JMAP EmailSubmission/query failed: missing EmailSubmission/query response in method_responses"
    )]
    MissingQueryResponse,
    /// The response carried no get response.
    #[error(
        "JMAP EmailSubmission/query failed: missing EmailSubmission/get response in method_responses"
    )]
    MissingGetResponse,
    /// The inner send coroutine failed.
    #[error("JMAP EmailSubmission/query failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP EmailSubmission/query failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The query response could not be parsed.
    #[error("JMAP EmailSubmission/query failed: parse EmailSubmission/query response: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    /// The get response could not be parsed.
    #[error("JMAP EmailSubmission/query failed: parse EmailSubmission/get response: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    /// The server returned a method-level error for the query call.
    #[error("JMAP EmailSubmission/query failed: EmailSubmission/query: {0}")]
    QueryMethod(JmapMethodError),
    /// The server returned a method-level error for the get call.
    #[error("JMAP EmailSubmission/query failed: EmailSubmission/get: {0}")]
    GetMethod(JmapMethodError),
}

/// Options for [`JmapEmailSubmissionQuery::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapEmailSubmissionQueryOptions {
    /// The filter conditions the submissions must match.
    pub filter: Option<JmapEmailSubmissionFilter>,
    /// The sort comparators applied to the results.
    pub sort: Option<Vec<JmapEmailSubmissionComparator>>,
    /// Zero-based index of the first result to return.
    pub position: Option<u64>,
    /// Maximum number of results to return.
    pub limit: Option<u64>,
}

/// Successful terminal output of [`JmapEmailSubmissionQuery`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionQueryOutput {
    /// The fetched email submissions.
    pub submissions: Vec<JmapEmailSubmission>,
    /// The total number of matching objects, when the server computed it.
    pub total: Option<u64>,
    /// Zero-based index of the first returned id.
    pub position: u64,
    /// The state the query results were computed at.
    pub query_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for batched `EmailSubmission/query` +
/// `EmailSubmission/get`.
pub struct JmapEmailSubmissionQuery {
    state: State,
}

impl JmapEmailSubmissionQuery {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapEmailSubmissionQueryOptions,
    ) -> Result<Self, JmapEmailSubmissionQueryError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_MAIL_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let query_args = SubmissionQueryArgs {
            account_id: &account_id,
            filter: opts.filter.as_ref(),
            sort: opts.sort.as_deref(),
            position: opts.position,
            limit: opts.limit,
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
            ids_ref: JmapResultReference {
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
            JMAP_CORE_CAPABILITY.into(),
            JMAP_MAIL_CAPABILITY.into(),
            JMAP_SUBMISSION_CAPABILITY.into(),
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmissionQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a JmapEmailSubmissionFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<&'a [JmapEmailSubmissionComparator]>,
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
    ids_ref: JmapResultReference<'a>,
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
    list: Vec<JmapEmailSubmission>,
}
