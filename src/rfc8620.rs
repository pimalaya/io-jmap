//! RFC 8620: The JSON Meta Application Protocol (JMAP).

pub mod blob_download;
pub mod blob_upload;
pub mod changes;
pub mod coroutine;
pub mod error;
pub mod event_source;
pub mod filter;
pub mod get;
pub mod push_subscription;
pub mod query;
pub mod query_changes;
pub mod request;
pub mod send;
pub mod session;
pub mod session_get;
pub mod set;

/// Core JMAP capability (RFC 8620).
pub const JMAP_CORE_CAPABILITY: &str = "urn:ietf:params:jmap:core";
