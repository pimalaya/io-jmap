//! RFC 8620 — The JSON Meta Application Protocol (JMAP).

#[path = "blob-download.rs"]
pub mod blob_download;
#[path = "blob-upload.rs"]
pub mod blob_upload;
pub mod changes;
pub mod error;
pub mod get;
pub mod query;
#[path = "query-changes.rs"]
pub mod query_changes;
pub(crate) mod result_reference;
pub mod send;
pub mod session;
#[path = "session-get.rs"]
pub mod session_get;
pub mod set;
