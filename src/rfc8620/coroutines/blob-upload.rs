//! I/O-free coroutine for uploading a blob (RFC 8620 §6.1).

use http::{
    header::{AUTHORIZATION, CONTENT_TYPE},
    Method,
};
use io_http::v1_1::coroutines::send::{SendHttp, SendHttpError, SendHttpResult};
use io_stream::io::StreamIo;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapBlobUploadError {
    #[error("Build HTTP request error: {0}")]
    BuildHttp(#[from] http::Error),
    #[error("Send HTTP request error: {0}")]
    SendHttp(#[from] SendHttpError),
    #[error("Parse blob upload response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP blob upload returned HTTP {0}")]
    HttpStatus(u16),
}

/// Result returned by the [`JmapBlobUpload`] coroutine.
#[derive(Debug)]
pub enum JmapBlobUploadResult {
    Ok {
        blob_id: String,
        blob_type: String,
        size: u64,
        keep_alive: bool,
    },
    Io {
        io: StreamIo,
    },
    Err {
        err: JmapBlobUploadError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobUploadResponse {
    blob_id: String,
    r#type: String,
    size: u64,
}

/// I/O-free coroutine for uploading a blob to a JMAP server.
///
/// POSTs raw bytes to `upload_url` (RFC 8620 §6.1).
/// The caller is responsible for resolving the URL template and opening
/// a stream to the correct host before driving this coroutine.
pub struct JmapBlobUpload {
    send: SendHttp,
}

impl JmapBlobUpload {
    /// Creates a new coroutine.
    ///
    /// - `upload_url`: the fully resolved upload URL (no template placeholders)
    /// - `content_type`: MIME type of the blob (e.g. `"message/rfc822"`)
    /// - `data`: raw bytes to upload
    pub fn new(
        http_auth: &SecretString,
        upload_url: &Url,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<Self, JmapBlobUploadError> {
        let host = upload_url.host_str().unwrap_or("localhost");
        let path_and_query = format!(
            "{}{}",
            upload_url.path(),
            upload_url
                .query()
                .map(|q| format!("?{q}"))
                .unwrap_or_default()
        );

        let http_request = http::Request::builder()
            .method(Method::POST)
            .uri(&path_and_query)
            .header("Host", host)
            .header(CONTENT_TYPE, content_type)
            .header(AUTHORIZATION, http_auth.expose_secret())
            .body(data)?;

        Ok(Self {
            send: SendHttp::new(http_request),
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapBlobUploadResult {
        let ok = match self.send.resume(arg) {
            SendHttpResult::Ok(ok) => ok,
            SendHttpResult::Io(io) => return JmapBlobUploadResult::Io { io },
            SendHttpResult::Err(err) => {
                return JmapBlobUploadResult::Err { err: err.into() };
            }
        };

        if !ok.response.status().is_success() {
            return JmapBlobUploadResult::Err {
                err: JmapBlobUploadError::HttpStatus(ok.response.status().as_u16()),
            };
        }

        match serde_json::from_slice::<BlobUploadResponse>(ok.response.body()) {
            Ok(r) => JmapBlobUploadResult::Ok {
                blob_id: r.blob_id,
                blob_type: r.r#type,
                size: r.size,
                keep_alive: ok.keep_alive,
            },
            Err(err) => JmapBlobUploadResult::Err {
                err: JmapBlobUploadError::ParseResponse(err),
            },
        }
    }
}
