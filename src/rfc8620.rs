//! RFC 8620: The JSON Meta Application Protocol (JMAP).

use alloc::{string::String, vec::Vec};

use serde::Deserialize;

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

/// Value of the core capability in the session's `capabilities` (RFC
/// 8620 §2): the server's limits, each `None` when the server left it
/// out.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapCoreCapability {
    /// Max size of one uploaded blob, in octets.
    #[serde(default)]
    pub max_size_upload: Option<u64>,
    /// Max number of uploads the server takes at once.
    #[serde(default)]
    pub max_concurrent_upload: Option<u64>,
    /// Max size of one API request, in octets.
    #[serde(default)]
    pub max_size_request: Option<u64>,
    /// Max number of API requests the server takes at once.
    #[serde(default)]
    pub max_concurrent_requests: Option<u64>,
    /// Max number of method calls in one API request.
    #[serde(default)]
    pub max_calls_in_request: Option<u64>,
    /// Max number of objects one `Foo/get` may fetch.
    #[serde(default)]
    pub max_objects_in_get: Option<u64>,
    /// Max number of objects one `Foo/set` may change.
    #[serde(default)]
    pub max_objects_in_set: Option<u64>,
    /// The collation algorithms the server sorts and filters with (RFC
    /// 4790).
    #[serde(default)]
    pub collation_algorithms: Vec<String>,
}
