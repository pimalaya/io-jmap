//! I/O-free coroutine for uploading a blob (RFC 8620 §6.1).

use http::{
    header::{AUTHORIZATION, CONTENT_TYPE},
    Method,
};
use io_http::v1_1::coroutines::send::{SendHttp, SendHttpError, SendHttpResult};
use io_stream::io::StreamIo;
use serde::Deserialize;
use secrecy::ExposeSecret;
use thiserror::Error;
use url::Url;

use crate::context::JmapContext;

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum UploadJmapBlobError {
    #[error("Build HTTP request error: {0}")]
    BuildHttp(#[from] http::Error),
    #[error("Send HTTP request error: {0}")]
    SendHttp(#[from] SendHttpError),
    #[error("Parse blob upload response error: {0}")]
    ParseResponse(#[source] serde_json::Error),
    #[error("JMAP blob upload returned HTTP {0}")]
    HttpStatus(u16),
}

/// Result returned by the [`UploadJmapBlob`] coroutine.
#[derive(Debug)]
pub enum UploadJmapBlobResult {
    Ok {
        context: JmapContext,
        blob_id: String,
        blob_type: String,
        size: u64,
        keep_alive: bool,
    },
    Io(StreamIo),
    Err {
        context: JmapContext,
        err: UploadJmapBlobError,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobUploadResponse {
    blob_id: String,
    #[serde(rename = "type")]
    blob_type: String,
    size: u64,
}

/// I/O-free coroutine for uploading a blob to a JMAP server.
///
/// POSTs raw bytes to `upload_url` (RFC 8620 §6.1).
/// The caller is responsible for resolving the URL template and opening
/// a stream to the correct host before driving this coroutine.
pub struct UploadJmapBlob {
    context: Option<JmapContext>,
    send: SendHttp,
}

impl UploadJmapBlob {
    /// Creates a new coroutine.
    ///
    /// - `upload_url`: the fully resolved upload URL (no template placeholders)
    /// - `content_type`: MIME type of the blob (e.g. `"message/rfc822"`)
    /// - `data`: raw bytes to upload
    pub fn new(
        context: JmapContext,
        upload_url: &Url,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<Self, UploadJmapBlobError> {
        let host = upload_url.host_str().unwrap_or("localhost");
        let path_and_query = format!(
            "{}{}",
            upload_url.path(),
            upload_url.query().map(|q| format!("?{q}")).unwrap_or_default()
        );

        let mut builder = http::Request::builder()
            .method(Method::POST)
            .uri(&path_and_query)
            .header("Host", host)
            .header(CONTENT_TYPE, content_type);

        if let Some(auth) = &context.http_auth {
            builder = builder.header(AUTHORIZATION, auth.expose_secret());
        }

        let http_request = builder.body(data)?;

        Ok(Self {
            context: Some(context),
            send: SendHttp::new(http_request),
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> UploadJmapBlobResult {
        let ok = match self.send.resume(arg) {
            SendHttpResult::Ok(ok) => ok,
            SendHttpResult::Io(io) => return UploadJmapBlobResult::Io(io),
            SendHttpResult::Err(err) => {
                let context = self.context.take().unwrap_or_default();
                return UploadJmapBlobResult::Err { context, err: err.into() };
            }
        };

        let context = self.context.take().unwrap_or_default();

        if !ok.response.status().is_success() {
            return UploadJmapBlobResult::Err {
                context,
                err: UploadJmapBlobError::HttpStatus(ok.response.status().as_u16()),
            };
        }

        match serde_json::from_slice::<BlobUploadResponse>(ok.response.body()) {
            Ok(r) => UploadJmapBlobResult::Ok {
                context,
                blob_id: r.blob_id,
                blob_type: r.blob_type,
                size: r.size,
                keep_alive: ok.keep_alive,
            },
            Err(err) => UploadJmapBlobResult::Err {
                context,
                err: UploadJmapBlobError::ParseResponse(err),
            },
        }
    }
}
