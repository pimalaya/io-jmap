//! I/O-free coroutine for downloading a blob (RFC 8620 §6.2).

use alloc::vec::Vec;

use io_http::{
    rfc9110::request::HttpRequest,
    rfc9112::send::{Http11Send, Http11SendError, Http11SendResult},
};
use log::trace;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use url::Url;

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapBlobDownloadError {
    #[error("Send HTTP request error: {0}")]
    SendHttp(#[from] Http11SendError),
    #[error("JMAP blob download returned HTTP {0}")]
    HttpStatus(u16),
}

/// Result returned by the [`JmapBlobDownload`] coroutine.
#[derive(Debug)]
pub enum JmapBlobDownloadResult {
    /// The coroutine has successfully completed.
    Ok { data: Vec<u8>, keep_alive: bool },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The server responded with a redirect to a new URL.
    ///
    /// The caller must open a new connection to the redirected URL and
    /// create a new [`JmapBlobDownload`] coroutine targeting it.
    WantsRedirect {
        url: Url,
        keep_alive: bool,
        same_origin: bool,
    },
    /// The coroutine encountered an error.
    Err(JmapBlobDownloadError),
}

/// I/O-free coroutine for downloading a blob from a JMAP server.
///
/// GETs from `download_url` (RFC 8620 §6.2).
/// The caller is responsible for resolving the URL template and opening
/// a stream to the correct host before driving this coroutine.
pub struct JmapBlobDownload {
    send: Http11Send,
}

impl JmapBlobDownload {
    /// Creates a new coroutine.
    ///
    /// - `download_url`: the fully resolved download URL (no template placeholders)
    pub fn new(http_auth: &SecretString, download_url: &Url) -> Self {
        let host = download_url.host_str().unwrap_or("localhost");

        let http_request = HttpRequest::get(download_url.clone())
            .header("Host", host)
            .header("Authorization", http_auth.expose_secret());

        trace!("download JMAP blob from {download_url}");

        Self {
            send: Http11Send::new(http_request),
        }
    }

    /// Advances the coroutine.
    ///
    /// Pass [`None`] when there is no data to provide (initial call,
    /// after a write). Pass `Some(data)` with bytes read from the
    /// socket after a [`JmapBlobDownloadResult::WantsRead`]. Pass
    /// `Some(&[])` to signal EOF.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapBlobDownloadResult {
        match self.send.resume(arg) {
            Http11SendResult::Ok {
                response,
                keep_alive,
                ..
            } => {
                if !response.status.is_success() {
                    let err = JmapBlobDownloadError::HttpStatus(*response.status);
                    return JmapBlobDownloadResult::Err(err);
                }

                JmapBlobDownloadResult::Ok {
                    data: response.body,
                    keep_alive,
                }
            }
            Http11SendResult::WantsRead => JmapBlobDownloadResult::WantsRead,
            Http11SendResult::WantsWrite(bytes) => JmapBlobDownloadResult::WantsWrite(bytes),
            Http11SendResult::WantsRedirect {
                url,
                keep_alive,
                same_origin,
                ..
            } => {
                trace!("blob download redirect to {url}; caller must reconnect");
                JmapBlobDownloadResult::WantsRedirect {
                    url,
                    keep_alive,
                    same_origin,
                }
            }
            Http11SendResult::Err(err) => JmapBlobDownloadResult::Err(err.into()),
        }
    }
}
