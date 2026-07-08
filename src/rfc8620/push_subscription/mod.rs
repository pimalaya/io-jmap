//! JMAP PushSubscription (RFC 8620 §7.2): register a URL the JMAP server
//! POSTs push messages to, verified via a pushed [`JmapPushVerification`]
//! code.
//!
//! Unlike other JMAP objects, push subscriptions are tied to authentication
//! credentials rather than accounts, so `PushSubscription/get` and
//! `PushSubscription/set` take no `accountId` and track no state string.

mod types;
#[doc(inline)]
pub use types::*;

pub mod get;
pub mod set;
