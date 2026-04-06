//! I/O-free coroutine for downloading a blob (RFC 8620 §6.2).

use alloc::vec::Vec;
use io_http::{
    rfc9110::request::HttpRequest,
    rfc9112::send::{Http11Send, Http11SendError, Http11SendResult},
};
use io_socket::io::{SocketInput, SocketOutput};
use secrecy::ExposeSecret;
use secrecy::SecretString;
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
    Ok { data: Vec<u8>, keep_alive: bool },
    Io { input: SocketInput },
    Err { err: JmapBlobDownloadError },
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

        Self {
            send: Http11Send::new(http_request),
        }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapBlobDownloadResult {
        match self.send.resume(arg) {
            Http11SendResult::Ok {
                response,
                keep_alive,
                ..
            } => {
                if !response.status.is_success() {
                    return JmapBlobDownloadResult::Err {
                        err: JmapBlobDownloadError::HttpStatus(*response.status),
                    };
                }

                JmapBlobDownloadResult::Ok {
                    data: response.body,
                    keep_alive,
                }
            }
            Http11SendResult::Io { input } => JmapBlobDownloadResult::Io { input },
            Http11SendResult::Redirect { url, .. } => {
                log::info!("blob download redirect to {url}; caller must reconnect");
                JmapBlobDownloadResult::Err {
                    err: JmapBlobDownloadError::HttpStatus(302),
                }
            }
            Http11SendResult::Err { err } => JmapBlobDownloadResult::Err { err: err.into() },
        }
    }
}
