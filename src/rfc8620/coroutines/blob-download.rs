//! I/O-free coroutine for downloading a blob (RFC 8620 §6.2).

use http::{header::AUTHORIZATION, Method};
use io_http::v1_1::coroutines::send::{SendHttp, SendHttpError, SendHttpResult};
use io_stream::io::StreamIo;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use thiserror::Error;
use url::Url;

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapBlobDownloadError {
    #[error("Build HTTP request error: {0}")]
    BuildHttp(#[from] http::Error),
    #[error("Send HTTP request error: {0}")]
    SendHttp(#[from] SendHttpError),
    #[error("JMAP blob download returned HTTP {0}")]
    HttpStatus(u16),
}

/// Result returned by the [`JmapBlobDownload`] coroutine.
#[derive(Debug)]
pub enum JmapBlobDownloadResult {
    Ok { data: Vec<u8>, keep_alive: bool },
    Io { io: StreamIo },
    Err { err: JmapBlobDownloadError },
}

/// I/O-free coroutine for downloading a blob from a JMAP server.
///
/// GETs from `download_url` (RFC 8620 §6.2).
/// The caller is responsible for resolving the URL template and opening
/// a stream to the correct host before driving this coroutine.
pub struct JmapBlobDownload {
    send: SendHttp,
}

impl JmapBlobDownload {
    /// Creates a new coroutine.
    ///
    /// - `download_url`: the fully resolved download URL (no template placeholders)
    pub fn new(
        http_auth: &SecretString,
        download_url: &Url,
    ) -> Result<Self, JmapBlobDownloadError> {
        let host = download_url.host_str().unwrap_or("localhost");
        let path_and_query = format!(
            "{}{}",
            download_url.path(),
            download_url
                .query()
                .map(|q| format!("?{q}"))
                .unwrap_or_default()
        );

        let http_request = http::Request::builder()
            .method(Method::GET)
            .uri(&path_and_query)
            .header("Host", host)
            .header(AUTHORIZATION, http_auth.expose_secret())
            .body(vec![])?;

        Ok(Self {
            send: SendHttp::new(http_request),
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapBlobDownloadResult {
        let ok = match self.send.resume(arg) {
            SendHttpResult::Ok(ok) => ok,
            SendHttpResult::Io(io) => return JmapBlobDownloadResult::Io { io },
            SendHttpResult::Err(err) => {
                return JmapBlobDownloadResult::Err { err: err.into() };
            }
        };

        if !ok.response.status().is_success() {
            return JmapBlobDownloadResult::Err {
                err: JmapBlobDownloadError::HttpStatus(ok.response.status().as_u16()),
            };
        }

        JmapBlobDownloadResult::Ok {
            data: ok.response.into_body(),
            keep_alive: ok.keep_alive,
        }
    }
}
