//! JMAP VacationResponse types (RFC 8621 §8).

use serde::{Deserialize, Serialize};

/// A JMAP VacationResponse object (RFC 8621 §8.1).
///
/// There is exactly one VacationResponse object per account. Its `id`
/// is always `"singleton"`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VacationResponse {
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

/// Patch object for `VacationResponse/set` update (RFC 8621 §8).
///
/// Only `Some` fields are serialized; `None` fields are left unchanged.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VacationResponseUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_date: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_date: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_body: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_body: Option<String>,
}
