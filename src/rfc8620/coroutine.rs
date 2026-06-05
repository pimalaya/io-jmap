//! Shared yield enum emitted by the three redirect-capable RFC 8620 coroutines
//! (session-get, blob-download, blob-upload). Their HTTP/1.1 exchange may
//! surface a 3xx; the caller chooses whether to follow it or treat it as an
//! error.

use alloc::vec::Vec;

use io_http::rfc9110::send::HttpSendYield;
use url::Url;

/// Per-step yield for redirect-capable JMAP coroutines: standard I/O variants
/// plus [`Self::WantsRedirect`].
#[derive(Debug)]
pub enum JmapRedirectYield {
    /// Driver should read more bytes and feed them back on the next resume.
    WantsRead,
    /// Driver should write these bytes; the next resume typically takes `None`.
    WantsWrite(Vec<u8>),
    /// Server responded with a 3xx. The caller opens a new connection when
    /// `!keep_alive || !same_origin` and builds a fresh coroutine for `url`.
    WantsRedirect {
        /// Resolved redirect target (from the `Location` header).
        url: Url,
        /// Whether the server will keep the connection open.
        keep_alive: bool,
        /// Whether the redirect stays on the same scheme, host, and port.
        same_origin: bool,
    },
}

impl From<HttpSendYield> for JmapRedirectYield {
    fn from(y: HttpSendYield) -> Self {
        match y {
            HttpSendYield::WantsRead => Self::WantsRead,
            HttpSendYield::WantsWrite(bytes) => Self::WantsWrite(bytes),
            HttpSendYield::WantsRedirect {
                url,
                keep_alive,
                same_origin,
                ..
            } => Self::WantsRedirect {
                url,
                keep_alive,
                same_origin,
            },
        }
    }
}
