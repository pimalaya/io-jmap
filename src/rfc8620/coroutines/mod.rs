//! RFC 8620 (JMAP core) coroutines.

#[path = "blob-download.rs"]
pub mod blob_download;
#[path = "blob-upload.rs"]
pub mod blob_upload;
pub mod changes;
pub mod get;
pub mod query;
#[path = "query-changes.rs"]
pub mod query_changes;
pub mod send;
#[path = "session-get.rs"]
pub mod session_get;
pub mod set;
