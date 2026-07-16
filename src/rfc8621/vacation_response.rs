//! JMAP for Mail: VacationResponse (RFC 8621 §8).

use alloc::string::String;

use serde::{Deserialize, Serialize};

pub mod get;
pub mod set;

/// JMAP for Vacation Response capability (RFC 8621 §8).
pub const JMAP_VACATION_RESPONSE_CAPABILITY: &str = "urn:ietf:params:jmap:vacationresponse";

/// A JMAP VacationResponse object (RFC 8621 §8.1).
///
/// There is exactly one VacationResponse object per account. Its `id`
/// is always `"singleton"`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapVacationResponse {
    /// Always `"singleton"`.
    pub id: String,
    /// Whether the vacation response is currently enabled.
    pub is_enabled: bool,
    /// Date/time (RFC 3339) from which the vacation response is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_date: Option<String>,
    /// Date/time (RFC 3339) until which the vacation response is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_date: Option<String>,
    /// Subject of the auto-reply message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Plaintext body of the auto-reply message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_body: Option<String>,
    /// HTML body of the auto-reply message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_body: Option<String>,
}
