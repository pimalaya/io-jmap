//! I/O-free coroutine for uploading a blob (RFC 8620 §6.1).

use alloc::{string::String, vec::Vec};

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
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::coroutine::*;
use crate::rfc8620::redirect::JmapRedirectYield;

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

/// Successful terminal output of [`JmapBlobUpload`].
#[derive(Clone, Debug)]
pub struct JmapBlobUploadOutput {
    pub blob_id: String,
    pub blob_type: String,
    pub size: u64,
    pub keep_alive: bool,
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
/// POSTs raw bytes to `upload_url` (RFC 8620 §6.1). The caller is
/// responsible for resolving the URL template and opening a stream to
/// the correct host before driving this coroutine.
///
/// A 3xx response yields [`JmapRedirectYield::WantsRedirect`]; the
/// caller must open a new connection to the redirect target and build a
/// fresh coroutine.
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

        trace!("upload JMAP blob to {upload_url}");

        Self {
            send: Http11Send::new(http_request),
        }
    }
}

impl JmapCoroutine for JmapBlobUpload {
    type Yield = JmapRedirectYield;
    type Return = Result<JmapBlobUploadOutput, JmapBlobUploadError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.send.resume(arg) {
            HttpCoroutineState::Complete(Ok(HttpSendOutput {
                response,
                keep_alive,
                ..
            })) => {
                if !response.status.is_success() {
                    let err = JmapBlobUploadError::HttpStatus(*response.status);
                    return JmapCoroutineState::Complete(Err(err));
                }

                match serde_json::from_slice::<BlobUploadResponse>(&response.body) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapBlobUploadOutput {
                        blob_id: r.blob_id,
                        blob_type: r.r#type,
                        size: r.size,
                        keep_alive,
                    })),
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapBlobUploadError::ParseResponse(err)))
                    }
                }
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
                trace!("blob upload redirect to {url}; caller must reconnect");
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
