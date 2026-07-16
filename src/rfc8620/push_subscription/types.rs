//! JMAP PushSubscription data types (RFC 8620 §7.2): the subscription object,
//! its create/update shapes, Web Push encryption keys, and the
//! PushVerification payload.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

/// A JMAP PushSubscription object (RFC 8620 §7.2), as returned by the server.
///
/// The `url` and `keys` properties are write-only (RFC 8620 §7.2.1: the
/// server MUST NOT return them), so they live on
/// [`JmapPushSubscriptionCreate`] only.
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

/// A partial [`JmapPushSubscription`] for `PushSubscription/set` create
/// requests.
///
/// `verificationCode` MUST NOT be set on create (RFC 8620 §7.2): the server
/// pushes a [`JmapPushVerification`] to `url` and the client copies the code
/// back via [`JmapPushSubscriptionUpdate`].
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapPushSubscriptionCreate {
    /// An ID unique to the client + device, containing no unobfuscated
    /// device ID (RFC 8620 §7.2).
    pub device_client_id: String,
    /// Absolute `https://` URL the server will POST push messages to.
    pub url: String,
    /// Client-generated encryption keys; when supplied, the server MUST
    /// encrypt all pushed data with them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<JmapPushSubscriptionKeys>,
    /// RFC 3339 expiry time; the server may clamp it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// Type names to restrict pushes to; `None` pushes all types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
}

/// Patch object for `PushSubscription/set` update requests; only `Some`
/// fields are serialized.
///
/// `url` and `keys` are immutable (RFC 8620 §7.2.2): to change them, destroy
/// the subscription and create a new one.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapPushSubscriptionUpdate {
    /// The code from the pushed [`JmapPushVerification`]; an invalid code is
    /// rejected with an `invalidProperties` set error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_code: Option<String>,
    /// New RFC 3339 expiry time extending (or shortening) the subscription
    /// lifetime; the server may clamp it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// Type names to restrict pushes to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
}

/// Client-generated Web Push encryption keys (RFC 8620 §7.2), both encoded
/// in URL-safe base64 as specified by RFC 8291.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JmapPushSubscriptionKeys {
    /// The P-256 ECDH public key.
    pub p256dh: String,
    /// The authentication secret.
    pub auth: String,
}

/// The PushVerification object the server POSTs to the subscription URL
/// right after create (RFC 8620 §7.2.2); the client MUST copy
/// `verification_code` into a [`JmapPushSubscriptionUpdate`] before the
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
