//! Shared yield type for redirect-capable JMAP coroutines.
//!
//! [`JmapSessionGet`](crate::rfc8620::session_get::JmapSessionGet),
//! [`JmapBlobDownload`](crate::rfc8620::blob_download::JmapBlobDownload)
//! and [`JmapBlobUpload`](crate::rfc8620::blob_upload::JmapBlobUpload)
//! all run a one-shot HTTP/1.1 exchange whose 3xx response surfaces as
//! an intermediate yield: the caller chooses whether to follow the
//! redirect (open a new connection if needed, build a fresh coroutine
//! targeting `url`) or surface it as an error.

use alloc::vec::Vec;

use url::Url;

/// Per-step yield emitted by the three redirect-capable JMAP
/// coroutines.
///
/// Extends the standard [`JmapYield`](crate::coroutine::JmapYield)
/// with the [`Self::WantsRedirect`] variant.
#[derive(Debug)]
pub enum JmapRedirectYield {
    /// Driver should read more bytes from the socket and feed them
    /// back on the next resume.
    WantsRead,
    /// Driver should write these bytes to the socket; the next resume
    /// typically takes `None`.
    WantsWrite(Vec<u8>),
    /// Server responded with a 3xx redirect.
    ///
    /// The caller is responsible for opening a new connection (when
    /// `!keep_alive || !same_origin`) and building a fresh coroutine
    /// targeting `url`.
    WantsRedirect {
        /// Resolved redirect target URL (from the `Location` header).
        url: Url,
        /// Whether the server indicated it will keep the connection
        /// open.
        keep_alive: bool,
        /// Whether the redirect stays on the same scheme, host, and
        /// port.
        same_origin: bool,
    },
}
