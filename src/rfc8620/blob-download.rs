//! I/O-free coroutine for downloading a blob (RFC 8620 §6.2).

use alloc::vec::Vec;

use io_http::{
    coroutine::*,
    rfc9110::{
        request::HttpRequest,
        send::{HttpSendOutput, HttpSendYield},
    },
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::trace;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use url::Url;

use crate::coroutine::*;
use crate::rfc8620::redirect::JmapRedirectYield;

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapBlobDownloadError {
    #[error("Send HTTP request error: {0}")]
    SendHttp(#[from] Http11SendError),
    #[error("JMAP blob download returned HTTP {0}")]
    HttpStatus(u16),
}

/// Successful terminal output of [`JmapBlobDownload`].
#[derive(Clone, Debug)]
pub struct JmapBlobDownloadOutput {
    pub data: Vec<u8>,
    pub keep_alive: bool,
}

/// I/O-free coroutine for downloading a blob from a JMAP server.
///
/// GETs from `download_url` (RFC 8620 §6.2). The caller is responsible
/// for resolving the URL template and opening a stream to the correct
/// host before driving this coroutine.
///
/// A 3xx response yields [`JmapRedirectYield::WantsRedirect`]; the
/// caller must open a new connection to the redirect target and build a
/// fresh coroutine.
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
}

impl JmapCoroutine for JmapBlobDownload {
    type Yield = JmapRedirectYield;
    type Return = Result<JmapBlobDownloadOutput, JmapBlobDownloadError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.send.resume(arg) {
            HttpCoroutineState::Complete(Ok(HttpSendOutput {
                response,
                keep_alive,
                ..
            })) => {
                if !response.status.is_success() {
                    let err = JmapBlobDownloadError::HttpStatus(*response.status);
                    return JmapCoroutineState::Complete(Err(err));
                }

                JmapCoroutineState::Complete(Ok(JmapBlobDownloadOutput {
                    data: response.body,
                    keep_alive,
                }))
            }
            HttpCoroutineState::Yielded(HttpSendYield::WantsRead) => {
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsRead)
            }
            HttpCoroutineState::Yielded(HttpSendYield::WantsWrite(bytes)) => {
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsWrite(bytes))
            }
            HttpCoroutineState::Yielded(HttpSendYield::WantsRedirect {
                url,
                keep_alive,
                same_origin,
                ..
            }) => {
                trace!("blob download redirect to {url}; caller must reconnect");
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsRedirect {
                    url,
                    keep_alive,
                    same_origin,
                })
            }
            HttpCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
        }
    }
}
