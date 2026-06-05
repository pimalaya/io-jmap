//! JMAP blob download coroutine (RFC 8620 §6.2): GETs from the caller-resolved
//! `download_url` and returns the response body bytes.
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
//!     rfc8620::{blob_download::JmapBlobDownload, coroutine::JmapRedirectYield},
//! };
//! use secrecy::SecretString;
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let url: Url = "https://api.example.com/jmap/download/a1/b1/blob".parse().unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine = JmapBlobDownload::new(&auth, &url);
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
//! println!("{} bytes", out.data.len());
//! ```

use core::fmt;

use alloc::vec::Vec;

use io_http::{
    coroutine::*,
    rfc9110::{request::HttpRequest, send::HttpSendOutput},
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::trace;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use url::Url;

use crate::{coroutine::*, rfc8620::coroutine::JmapRedirectYield};

/// Failure causes during the JMAP blob-download flow.
#[derive(Debug, Error)]
pub enum JmapBlobDownloadError {
    #[error("JMAP blob-download failed: HTTP {0}")]
    HttpStatus(u16),
    #[error("JMAP blob-download failed: {0}")]
    Send(#[from] Http11SendError),
}

/// Successful terminal output of [`JmapBlobDownload`].
#[derive(Clone, Debug)]
pub struct JmapBlobDownloadOutput {
    pub data: Vec<u8>,
    pub keep_alive: bool,
}

/// I/O-free coroutine downloading a blob from a JMAP server (RFC 8620 §6.2).
pub struct JmapBlobDownload {
    state: State,
}

impl JmapBlobDownload {
    /// `download_url` is the fully resolved download URL (no template
    /// placeholders).
    pub fn new(http_auth: &SecretString, download_url: &Url) -> Self {
        let host = download_url.host_str().unwrap_or("localhost");

        let http_request = HttpRequest::get(download_url.clone())
            .header("Host", host)
            .header("Authorization", http_auth.expose_secret());

        trace!("download JMAP blob from {download_url}");

        Self {
            state: State::Send(Http11Send::new(http_request)),
        }
    }
}

impl JmapCoroutine for JmapBlobDownload {
    type Yield = JmapRedirectYield;
    type Return = Result<JmapBlobDownloadOutput, JmapBlobDownloadError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("blob-download: {}", self.state);
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
                        let err = JmapBlobDownloadError::HttpStatus(*response.status);
                        return JmapCoroutineState::Complete(Err(err));
                    }

                    JmapCoroutineState::Complete(Ok(JmapBlobDownloadOutput {
                        data: response.body,
                        keep_alive,
                    }))
                }
            },
        }
    }
}

enum State {
    Send(Http11Send),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;

    use super::*;

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_url() -> Url {
        "https://api.example.com/jmap/download/a1/b1/blob"
            .parse()
            .unwrap()
    }

    fn build_http_reply(status: u16, body: &[u8]) -> Vec<u8> {
        let head = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: {}\r\n\r\n",
            status,
            body.len()
        );
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }

    #[test]
    fn success_returns_ok() {
        let mut cor = JmapBlobDownload::new(&make_auth(), &make_url());

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(200, b"hello blob");
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.data, b"hello blob");
    }

    #[test]
    fn http_error_returns_status() {
        let mut cor = JmapBlobDownload::new(&make_auth(), &make_url());

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
        let err = expect_complete_err(&mut cor, reply);
        assert!(matches!(err, JmapBlobDownloadError::HttpStatus(404)));
    }

    #[test]
    fn redirect_yields_redirect() {
        let mut cor = JmapBlobDownload::new(&make_auth(), &make_url());

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 302 Found\r\nLocation: https://cdn.example.com/blob\r\nContent-Length: 0\r\n\r\n";
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Yielded(JmapRedirectYield::WantsRedirect { url, .. }) => {
                assert_eq!(url.host_str(), Some("cdn.example.com"));
            }
            state => panic!("expected WantsRedirect, got {state:?}"),
        }
    }

    #[test]
    fn empty_body_succeeds() {
        let mut cor = JmapBlobDownload::new(&make_auth(), &make_url());

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(200, b"");
        let out = expect_complete_ok(&mut cor, &reply);
        assert!(out.data.is_empty());
    }

    #[test]
    fn request_uses_get_method() {
        let mut cor = JmapBlobDownload::new(&make_auth(), &make_url());
        let bytes = expect_wants_write(&mut cor, None);
        let req = core::str::from_utf8(&bytes).expect("utf8 request");
        assert!(req.starts_with("GET "));
    }

    // --- utils

    fn expect_wants_write(cor: &mut JmapBlobDownload, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapRedirectYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapBlobDownload) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapRedirectYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut JmapBlobDownload, reply: &[u8]) -> JmapBlobDownloadOutput {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(cor: &mut JmapBlobDownload, reply: &[u8]) -> JmapBlobDownloadError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
