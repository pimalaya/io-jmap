//! JMAP blob upload coroutine (RFC 8620 §6.1): POSTs raw bytes to the
//! caller-resolved `upload_url` and returns the server-assigned blob id.
//!
//! A 3xx response surfaces as [`JmapRedirectYield::WantsRedirect`]; the caller
//! must open a new connection to the redirect target and build a fresh
//! coroutine.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_jmap::{
//!     coroutine::{JmapCoroutine, JmapCoroutineState},
//!     rfc8620::{blob_upload::JmapBlobUpload, coroutine::JmapRedirectYield},
//! };
//! use secrecy::SecretString;
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let url: Url = "https://api.example.com/jmap/upload/a1/".parse().unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let data = b"hello".to_vec();
//! let content_type = "application/octet-stream";
//! let mut coroutine =
//!     JmapBlobUpload::new(&auth, &url, content_type, data).unwrap();
//! let mut arg = None;
//!
//! let out = loop {
//!     match coroutine.resume(arg.take()) {
//!         JmapCoroutineState::Yielded(JmapRedirectYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         JmapCoroutineState::Yielded(JmapRedirectYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         JmapCoroutineState::Yielded(JmapRedirectYield::WantsRedirect { .. }) => {
//!             unimplemented!("open a new connection and start over");
//!         }
//!         JmapCoroutineState::Complete(Ok(out)) => break out,
//!         JmapCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("uploaded blob {}", out.blob_id);
//! ```

use alloc::{string::String, vec::Vec};

use io_http::{
    coroutine::*,
    rfc9110::{request::HttpRequest, send::HttpSendOutput},
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::{debug, trace};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::{coroutine::*, rfc8620::coroutine::JmapRedirectYield};

/// Failure causes during the JMAP blob-upload flow.
#[derive(Debug, Error)]
pub enum JmapBlobUploadError {
    /// The content type holds a CR, LF or NUL (RFC 9110 §5.5).
    #[error("JMAP blob-upload failed: invalid content type")]
    InvalidContentType,
    /// The server answered with a non-2xx status.
    #[error("JMAP blob-upload failed: HTTP {0}")]
    HttpStatus(u16),
    /// The inner HTTP/1.1 send coroutine failed.
    #[error("JMAP blob-upload failed: {0}")]
    Send(#[from] Http11SendError),
    /// The method response could not be parsed.
    #[error("JMAP blob-upload failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
}

/// Successful terminal output of [`JmapBlobUpload`].
#[derive(Clone, Debug)]
pub struct JmapBlobUploadOutput {
    /// The server-assigned blob id.
    pub blob_id: String,
    /// The media type of the blob, as detected by the server.
    pub blob_type: String,
    /// The size of the blob, in bytes.
    pub size: u64,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobUploadResponse {
    blob_id: String,
    r#type: String,
    size: u64,
}

/// I/O-free coroutine uploading a blob to a JMAP server (RFC 8620 §6.1).
pub struct JmapBlobUpload {
    state: State,
}

impl JmapBlobUpload {
    /// - `upload_url`: the fully resolved upload URL (no template placeholders)
    /// - `content_type`: MIME type of the blob (e.g. `"message/rfc822"`)
    /// - `data`: raw bytes to upload
    ///
    /// A `content_type` holding a CR, LF or NUL is refused.
    pub fn new(
        http_auth: &SecretString,
        upload_url: &Url,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<Self, JmapBlobUploadError> {
        if content_type.bytes().any(|b| matches!(b, b'\r' | b'\n' | 0)) {
            return Err(JmapBlobUploadError::InvalidContentType);
        }

        let mut http_request = HttpRequest::get(upload_url.clone())
            .header("Content-Type", content_type)
            .header("Authorization", http_auth.expose_secret())
            .body(data);
        http_request.method = "POST".into();

        debug!("prepare blob upload request");
        trace!("upload url: {upload_url}");

        Ok(Self {
            state: State::Send(Http11Send::new(http_request)),
        })
    }
}

impl JmapCoroutine for JmapBlobUpload {
    type Yield = JmapRedirectYield;
    type Return = Result<JmapBlobUploadOutput, JmapBlobUploadError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => match send.resume(arg) {
                HttpCoroutineState::Yielded(y) => JmapCoroutineState::Yielded(y.into()),
                HttpCoroutineState::Complete(Err(err)) => {
                    JmapCoroutineState::Complete(Err(err.into()))
                }
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
                        Err(err) => JmapCoroutineState::Complete(Err(
                            JmapBlobUploadError::ParseResponse(err),
                        )),
                    }
                }
            },
        }
    }
}

enum State {
    Send(Http11Send),
}

#[cfg(test)]
mod tests {
    use alloc::format;

    use crate::rfc8620::blob_upload::*;

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_url() -> Url {
        "https://api.example.com/jmap/upload/a1/".parse().unwrap()
    }

    fn build_http_reply(status: u16, body: &[u8]) -> Vec<u8> {
        let head = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
            status,
            body.len()
        );
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }

    #[test]
    fn success_returns_ok() {
        let mut cor = upload("text/plain").unwrap();

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{"accountId":"a1","blobId":"b1","type":"text/plain","size":2}"#;
        let reply = build_http_reply(200, body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.blob_id, "b1");
        assert_eq!(out.blob_type, "text/plain");
        assert_eq!(out.size, 2);
    }

    #[test]
    fn http_error_returns_status() {
        let mut cor = upload("text/plain").unwrap();

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 413 Payload Too Large\r\nContent-Length: 0\r\n\r\n";
        let err = expect_complete_err(&mut cor, reply);
        assert!(matches!(err, JmapBlobUploadError::HttpStatus(413)));
    }

    #[test]
    fn redirect_yields_redirect() {
        let mut cor = upload("text/plain").unwrap();

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 307 Temporary Redirect\r\nLocation: https://upload.example.com/jmap/upload/a1/\r\nContent-Length: 0\r\n\r\n";
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Yielded(JmapRedirectYield::WantsRedirect { url, .. }) => {
                assert_eq!(url.host_str(), Some("upload.example.com"));
            }
            state => panic!("expected WantsRedirect, got {state:?}"),
        }
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let mut cor = upload("text/plain").unwrap();

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(200, b"{nope");
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapBlobUploadError::ParseResponse(_)));
    }

    #[test]
    fn request_uses_post_method() {
        let mut cor = upload("text/plain").unwrap();
        let bytes = expect_wants_write(&mut cor, None);
        let req = core::str::from_utf8(&bytes).expect("utf8 request");
        assert!(req.starts_with("POST "));
    }

    #[test]
    fn request_host_keeps_the_port() {
        let url = "https://api.example.com:8443/jmap/upload/a1/";
        let mut cor = JmapBlobUpload::new(
            &make_auth(),
            &url.parse().unwrap(),
            "text/plain",
            Vec::new(),
        )
        .unwrap();
        let bytes = expect_wants_write(&mut cor, None);
        let req = core::str::from_utf8(&bytes).unwrap().to_ascii_lowercase();
        assert!(req.contains("\r\nhost: api.example.com:8443\r\n"), "{req}");
    }

    #[test]
    fn content_type_with_cr_lf_or_nul_returns_error() {
        for content_type in [
            "text/plain\r\nX-Injected: 1",
            "text/\rplain",
            "text/\nplain",
            "text/\0plain",
        ] {
            let err = upload(content_type).err();
            assert!(
                matches!(err, Some(JmapBlobUploadError::InvalidContentType)),
                "{content_type:?}: {err:?}"
            );
        }
    }

    fn upload(content_type: &str) -> Result<JmapBlobUpload, JmapBlobUploadError> {
        JmapBlobUpload::new(&make_auth(), &make_url(), content_type, b"hi".to_vec())
    }

    fn expect_wants_write(cor: &mut JmapBlobUpload, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapRedirectYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapBlobUpload) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapRedirectYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut JmapBlobUpload, reply: &[u8]) -> JmapBlobUploadOutput {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(cor: &mut JmapBlobUpload, reply: &[u8]) -> JmapBlobUploadError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
