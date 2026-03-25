//! I/O-free coroutine for downloading a blob (RFC 8620 §6.2).

use http::{header::AUTHORIZATION, Method};
use io_http::v1_1::coroutines::send::{SendHttp, SendHttpError, SendHttpResult};
use io_stream::io::StreamIo;
use secrecy::ExposeSecret;
use thiserror::Error;
use url::Url;

use crate::context::JmapContext;

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum DownloadJmapBlobError {
    #[error("Build HTTP request error: {0}")]
    BuildHttp(#[from] http::Error),
    #[error("Send HTTP request error: {0}")]
    SendHttp(#[from] SendHttpError),
    #[error("JMAP blob download returned HTTP {0}")]
    HttpStatus(u16),
}

/// Result returned by the [`DownloadJmapBlob`] coroutine.
#[derive(Debug)]
pub enum DownloadJmapBlobResult {
    Ok {
        context: JmapContext,
        data: Vec<u8>,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: DownloadJmapBlobError,
    },
}

/// I/O-free coroutine for downloading a blob from a JMAP server.
///
/// GETs from `download_url` (RFC 8620 §6.2).
/// The caller is responsible for resolving the URL template and opening
/// a stream to the correct host before driving this coroutine.
pub struct DownloadJmapBlob {
    context: Option<JmapContext>,
    send: SendHttp,
}

impl DownloadJmapBlob {
    /// Creates a new coroutine.
    ///
    /// - `download_url`: the fully resolved download URL (no template placeholders)
    pub fn new(
        context: JmapContext,
        download_url: &Url,
    ) -> Result<Self, DownloadJmapBlobError> {
        let host = download_url.host_str().unwrap_or("localhost");
        let path_and_query = format!(
            "{}{}",
            download_url.path(),
            download_url.query().map(|q| format!("?{q}")).unwrap_or_default()
        );

        let mut builder = http::Request::builder()
            .method(Method::GET)
            .uri(&path_and_query)
            .header("Host", host);

        if let Some(auth) = &context.http_auth {
            builder = builder.header(AUTHORIZATION, auth.expose_secret());
        }

        let http_request = builder.body(vec![])?;

        Ok(Self {
            context: Some(context),
            send: SendHttp::new(http_request),
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> DownloadJmapBlobResult {
        let ok = match self.send.resume(arg) {
            SendHttpResult::Ok(ok) => ok,
            SendHttpResult::Io(io) => return DownloadJmapBlobResult::Io(io),
            SendHttpResult::Err(err) => {
                let context = self.context.take().unwrap_or_default();
                return DownloadJmapBlobResult::Err { context, err: err.into() };
            }
        };

        let context = self.context.take().unwrap_or_default();

        if !ok.response.status().is_success() {
            return DownloadJmapBlobResult::Err {
                context,
                err: DownloadJmapBlobError::HttpStatus(ok.response.status().as_u16()),
            };
        }

        DownloadJmapBlobResult::Ok {
            context,
            data: ok.response.into_body(),
            keep_alive: ok.keep_alive,
        }
    }
}
