//! I/O-free coroutine for uploading a blob (RFC 8620 §6.1).

use alloc::{string::String, vec::Vec};
use io_http::{
    rfc9110::request::HttpRequest,
    rfc9112::send::{Http11Send, Http11SendError, Http11SendResult},
};
use io_socket::io::{SocketInput, SocketOutput};
use secrecy::ExposeSecret;
use secrecy::SecretString;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapBlobUploadError {
    #[error("Send HTTP request error: {0}")]
    SendHttp(#[from] Http11SendError),
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
        input: SocketInput,
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
    send: Http11Send,
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
    ) -> Self {
        let host = upload_url.host_str().unwrap_or("localhost");

        let mut http_request = HttpRequest::get(upload_url.clone())
            .header("Host", host)
            .header("Content-Type", content_type)
            .header("Authorization", http_auth.expose_secret())
            .body(data);
        http_request.method = "POST".into();

        Self {
            send: Http11Send::new(http_request),
        }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<SocketOutput>) -> JmapBlobUploadResult {
        match self.send.resume(arg) {
            Http11SendResult::Ok {
                response,
                keep_alive,
                ..
            } => {
                if !response.status.is_success() {
                    return JmapBlobUploadResult::Err {
                        err: JmapBlobUploadError::HttpStatus(*response.status),
                    };
                }

                match serde_json::from_slice::<BlobUploadResponse>(&response.body) {
                    Ok(r) => JmapBlobUploadResult::Ok {
                        blob_id: r.blob_id,
                        blob_type: r.r#type,
                        size: r.size,
                        keep_alive,
                    },
                    Err(err) => JmapBlobUploadResult::Err {
                        err: JmapBlobUploadError::ParseResponse(err),
                    },
                }
            }
            Http11SendResult::Io { input } => JmapBlobUploadResult::Io { input },
            Http11SendResult::Redirect { .. } => JmapBlobUploadResult::Err {
                err: JmapBlobUploadError::HttpStatus(302),
            },
            Http11SendResult::Err { err } => JmapBlobUploadResult::Err { err: err.into() },
        }
    }
}
