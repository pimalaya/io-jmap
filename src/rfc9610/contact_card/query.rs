//! Batched JMAP `ContactCard/query` + `ContactCard/get` coroutine (RFC 9610
//! §3.1 and §3.3): single HTTP request, server-side `#ids` back-reference
//! resolves the get against the query results.
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
//!     rfc9610::contact_card::query::{JmapContactCardQuery, JmapContactCardQueryOptions},
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
//!     "primaryAccounts": {"urn:ietf:params:jmap:contacts": "a1"},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine =
//!     JmapContactCardQuery::new(&session, &auth, JmapContactCardQueryOptions::default())
//!         .unwrap();
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
//! println!("{} cards", out.cards.len());
//! ```

use core::fmt;

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{
        JMAP_CORE_CAPABILITY, error::JmapMethodError, request::JmapBatch,
        request::JmapResultReference, send::*, session::JmapSession,
    },
    rfc9610::{JMAP_CONTACTS_CAPABILITY, contact_card::JmapContactCard},
};

/// Filter for `ContactCard/query` (RFC 9610 §3.3.1); all specified
/// conditions must apply.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapContactCardFilter {
    /// AddressBook id the card must be in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_address_book: Option<String>,
    /// Exact JSContact `uid` of the card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    /// Uid the card's `members` property must contain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_member: Option<String>,
    /// Exact JSContact `kind` of the card, e.g. `group`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The card's `created` date-time must be before this UTC date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_before: Option<String>,
    /// The card's `created` date-time must be the same or after this UTC
    /// date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_after: Option<String>,
    /// The card's `updated` date-time must be before this UTC date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_before: Option<String>,
    /// The card's `updated` date-time must be the same or after this UTC
    /// date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_after: Option<String>,
    /// Free-text match against any text in the card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Match against any NameComponent or the full name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Match against NameComponents of kind `given`.
    #[serde(rename = "name/given", skip_serializing_if = "Option::is_none")]
    pub name_given: Option<String>,
    /// Match against NameComponents of kind `surname`.
    #[serde(rename = "name/surname", skip_serializing_if = "Option::is_none")]
    pub name_surname: Option<String>,
    /// Match against NameComponents of kind `surname2`.
    #[serde(rename = "name/surname2", skip_serializing_if = "Option::is_none")]
    pub name_surname2: Option<String>,
    /// Match against any Nickname name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// Match against any Organization name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    /// Match against any EmailAddress address or label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Match against any Phone number or label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// Match against any OnlineService service, uri, user or label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_service: Option<String>,
    /// Match against any AddressComponent or the full address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// Match against any Note note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Sort property for `ContactCard/query` (RFC 9610 §3.3.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JmapContactCardSortProperty {
    /// The `created` date on the ContactCard.
    Created,
    /// The `updated` date on the ContactCard.
    Updated,
    /// The first NameComponent of kind `given`.
    NameGiven,
    /// The first NameComponent of kind `surname`.
    NameSurname,
    /// The first NameComponent of kind `surname2`.
    NameSurname2,
}

impl fmt::Display for JmapContactCardSortProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::NameGiven => "name/given",
            Self::NameSurname => "name/surname",
            Self::NameSurname2 => "name/surname2",
        })
    }
}

impl Serialize for JmapContactCardSortProperty {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Sort comparator for `ContactCard/query` (RFC 8620 §5.5).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapContactCardSortComparator {
    /// The property to sort by.
    pub property: JmapContactCardSortProperty,
    /// Ascending if `None` or `Some(true)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,
}

/// Failure causes during a batched JMAP `ContactCard/query` +
/// `ContactCard/get` flow.
#[derive(Debug, Error)]
pub enum JmapContactCardQueryError {
    /// The response carried no query response.
    #[error(
        "JMAP ContactCard/query failed: missing ContactCard/query response in method_responses"
    )]
    MissingQueryResponse,
    /// The response carried no get response.
    #[error("JMAP ContactCard/query failed: missing ContactCard/get response in method_responses")]
    MissingGetResponse,
    /// The inner send coroutine failed.
    #[error("JMAP ContactCard/query failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP ContactCard/query failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The query response could not be parsed.
    #[error("JMAP ContactCard/query failed: parse ContactCard/query response: {0}")]
    ParseQueryResponse(#[source] serde_json::Error),
    /// The get response could not be parsed.
    #[error("JMAP ContactCard/query failed: parse ContactCard/get response: {0}")]
    ParseGetResponse(#[source] serde_json::Error),
    /// The server returned a method-level error for the query call.
    #[error("JMAP ContactCard/query failed: ContactCard/query: {0}")]
    QueryMethod(JmapMethodError),
    /// The server returned a method-level error for the get call.
    #[error("JMAP ContactCard/query failed: ContactCard/get: {0}")]
    GetMethod(JmapMethodError),
}

/// Options for [`JmapContactCardQuery::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapContactCardQueryOptions {
    /// Filter criteria; `None` matches all cards.
    pub filter: Option<JmapContactCardFilter>,
    /// Sort order; `None` uses the server default.
    pub sort: Option<Vec<JmapContactCardSortComparator>>,
    /// Zero-based offset into the result list.
    pub position: Option<u64>,
    /// Max number of cards to return.
    pub limit: Option<u64>,
    /// Card properties to fetch (JSContact property names plus `id` and
    /// `addressBookIds`); `None` returns all.
    pub properties: Option<Vec<String>>,
}

/// Successful terminal output of [`JmapContactCardQuery`].
#[derive(Clone, Debug)]
pub struct JmapContactCardQueryOutput {
    /// The fetched contact cards.
    pub cards: Vec<JmapContactCard>,
    /// The total number of matching objects, when the server computed it.
    pub total: Option<u64>,
    /// Zero-based index of the first returned id.
    pub position: u64,
    /// The state the query results were computed at.
    pub query_state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine for the combined `ContactCard/query` +
/// `ContactCard/get` operation.
pub struct JmapContactCardQuery {
    state: State,
}

impl JmapContactCardQuery {
    /// Prepares the method call request and builds the coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        opts: JmapContactCardQueryOptions,
    ) -> Result<Self, JmapContactCardQueryError> {
        let account_id = session
            .primary_accounts
            .get(JMAP_CONTACTS_CAPABILITY)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        let query_args = ContactCardQueryArgs {
            account_id: &account_id,
            filter: opts.filter.as_ref(),
            sort: opts.sort.as_deref(),
            position: opts.position,
            limit: opts.limit,
            calculate_total: true,
        };

        let mut batch = JmapBatch::new();
        let query_id = batch.add(
            "ContactCard/query",
            serde_json::to_value(&query_args).map_err(JmapContactCardQueryError::SerializeArgs)?,
        );

        let get_args = ContactCardGetByRefArgs {
            account_id: &account_id,
            ids_ref: JmapResultReference {
                result_of: &query_id,
                name: "ContactCard/query",
                path: "/ids",
            },
            properties: opts.properties.as_deref(),
        };

        batch.add(
            "ContactCard/get",
            serde_json::to_value(&get_args).map_err(JmapContactCardQueryError::SerializeArgs)?,
        );

        let request = batch.into_request(vec![
            JMAP_CORE_CAPABILITY.into(),
            JMAP_CONTACTS_CAPABILITY.into(),
        ]);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
        })
    }
}

impl JmapCoroutine for JmapContactCardQuery {
    type Yield = JmapYield;
    type Return = Result<JmapContactCardQueryOutput, JmapContactCardQueryError>;

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
                        JmapContactCardQueryError::MissingQueryResponse,
                    ));
                };

                if query_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(query_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(
                        JmapContactCardQueryError::QueryMethod(err),
                    ));
                }

                let query_response =
                    match serde_json::from_value::<ContactCardQueryResponse>(query_args) {
                        Ok(r) => r,
                        Err(err) => {
                            return JmapCoroutineState::Complete(Err(
                                JmapContactCardQueryError::ParseQueryResponse(err),
                            ));
                        }
                    };

                let Some((get_name, get_args, _)) = responses.next() else {
                    return JmapCoroutineState::Complete(Err(
                        JmapContactCardQueryError::MissingGetResponse,
                    ));
                };

                if get_name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(get_args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(
                        JmapContactCardQueryError::GetMethod(err),
                    ));
                }

                match serde_json::from_value::<ContactCardGetResponse>(get_args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapContactCardQueryOutput {
                        cards: r.list,
                        total: query_response.total,
                        position: query_response.position,
                        query_state: query_response.query_state,
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(
                        JmapContactCardQueryError::ParseGetResponse(err),
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
struct ContactCardQueryArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a JmapContactCardFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<&'a [JmapContactCardSortComparator]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    calculate_total: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContactCardGetByRefArgs<'a> {
    account_id: &'a str,
    #[serde(rename = "#ids")]
    ids_ref: JmapResultReference<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [String]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContactCardQueryResponse {
    query_state: String,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    position: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContactCardGetResponse {
    list: Vec<JmapContactCard>,
}
