//! Batched JMAP `Email/query` + `Email/get` coroutine (RFC 8621 §4):
//! a single HTTP request that runs `Email/query` to find matching ids
//! and `Email/get` (via a Result Reference) to fetch their properties.
//!
//! Equivalent to IMAP's `SELECT` + `SEARCH` + `FETCH` but in one round
//! trip; the result reference (`#ids`) keeps the two method calls
//! linked server-side.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::{
//!     rfc8620::JmapSession,
//!     rfc8621::email::query::JmapEmailQuery,
//! };
//! use secrecy::SecretString;
//!
//! # fn demo(session: &JmapSession) {
//! let auth = SecretString::from("Bearer xyz");
//! let coroutine = JmapEmailQuery::new(
//!     session,
//!     &auth,
//!     None,
//!     None,
//!     Some(0),
//!     Some(20),
//!     None,
//! )
//! .unwrap();
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
    rfc8620::{
        CORE_CAPABILITY, Filter, JmapBatch, JmapMethodError, JmapSession, ResultReference, send::*,
    },
    rfc8621::{
        MAIL_CAPABILITY,
        email::{Email, EmailComparator, EmailFilter, EmailProperty},
    },
};

/// Failure causes during a batched JMAP `Email/query` + `Email/get` flow.
#[derive(Debug, Error)]
pub enum JmapEmailQueryError {
    #[error("JMAP Email/query failed: missing Email/query response in method_responses")]
    MissingQueryResponse,
    #[error("JMAP Email/query failed: missing Email/get response in method_responses")]
    MissingGetResponse,
    #[error("JMAP Email/query failed: {0}")]
    Send(#[from] JmapSendError),
    #[error("JMAP Email/query failed: parse Email/query response: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    #[error("JMAP Email/query failed: parse Email/get response: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    #[error("JMAP Email/query failed: Email/query: {0}")]
    QueryMethod(JmapMethodError),
    #[error("JMAP Email/query failed: Email/get: {0}")]
    GetMethod(JmapMethodError),
}

/// Successful terminal output of [`JmapEmailQuery`].
#[derive(Clone, Debug)]
pub struct JmapEmailQueryOutput {
    pub emails: Vec<Email>,
    pub total: Option<u64>,
    pub position: u64,
    pub query_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the combined `Email/query` + `Email/get` operation.
pub struct JmapEmailQuery {
    state: State,
}

impl JmapEmailQuery {
    /// - `filter`: filter criteria (pass `None` for all emails)
    /// - `sort`: sort order (pass `None` for default)
    /// - `position`: offset from the start of the results
    /// - `limit`: maximum number of emails to return
    /// - `properties`: email properties to fetch (pass `None` for all)
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        filter: Option<Filter<EmailFilter>>,
        sort: Option<Vec<EmailComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<Self, JmapEmailQueryError> {
        let account_id = session
            .primary_accounts
            .get(MAIL_CAPABILITY)
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

        let request = batch.into_request(vec![CORE_CAPABILITY.into(), MAIL_CAPABILITY.into()]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapEmailQuery {
    type Yield = JmapYield;
    type Return = Result<JmapEmailQueryOutput, JmapEmailQueryError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("Email/query: {}", self.state);
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

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a Filter<EmailFilter>>,
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
