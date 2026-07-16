//! JMAP PushSubscription (RFC 8620 §7.2): register a URL the JMAP server
//! POSTs push messages to, verified via a pushed [`JmapPushVerification`]
//! code.
//!
//! Unlike other JMAP objects, push subscriptions are tied to authentication
//! credentials rather than accounts, so `PushSubscription/get` and
//! `PushSubscription/set` take no `accountId` and track no state string.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

pub mod get;
pub mod set;

/// A JMAP PushSubscription object (RFC 8620 §7.2), as returned by the server.
///
/// The `url` and `keys` properties are write-only (RFC 8620 §7.2.1: the
/// server MUST NOT return them), so they live on
/// [`set::JmapPushSubscriptionCreate`] only.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapPushSubscription {
    /// The server-assigned ID. Defaults to empty in `PushSubscription/set`
    /// updated echoes, which carry only the server-changed properties.
    #[serde(default)]
    pub id: String,
    /// An ID unique to the client + device that created the subscription,
    /// letting clients recognize their own subscriptions after losing local
    /// state (RFC 8620 §7.2).
    #[serde(default)]
    pub device_client_id: Option<String>,
    /// The verification code proving the client controls the URL, copied by
    /// the client from the pushed [`JmapPushVerification`].
    #[serde(default)]
    pub verification_code: Option<String>,
    /// RFC 3339 time this subscription expires; the server may set or clamp
    /// it.
    #[serde(default)]
    pub expires: Option<String>,
    /// Type names pushes are restricted to; `None` pushes all types.
    #[serde(default)]
    pub types: Option<Vec<String>>,
}

/// The PushVerification object the server POSTs to the subscription URL
/// right after create (RFC 8620 §7.2.2); the client MUST copy
/// `verification_code` into a [`set::JmapPushSubscriptionUpdate`] before the
/// server makes any further pushes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapPushVerification {
    /// The type tag: always the string `PushVerification`.
    #[serde(rename = "@type", default = "default_type_tag")]
    pub r#type: String,
    /// The ID of the push subscription that was created.
    pub push_subscription_id: String,
    /// The code to copy back into the subscription.
    pub verification_code: String,
}

fn default_type_tag() -> String {
    "PushVerification".to_string()
}
