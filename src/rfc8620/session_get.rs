//! JMAP session discovery coroutine (RFC 8620 §2): GETs either
//! `/.well-known/jmap` (for a base URL) or the supplied URL directly, returning
//! the parsed [`JmapSession`].
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
//!     rfc8620::{coroutine::JmapRedirectYield, session_get::JmapSessionGet},
//! };
//! use secrecy::SecretString;
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//!
//! let mut buf = [0u8; 4096];
//!
//! let url: Url = "https://api.example.com/".parse().unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine = JmapSessionGet::new(&auth, &url);
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
//! println!("{:?}", out.session);
//! ```

use core::fmt;

use io_http::{
    coroutine::*,
    rfc9110::{request::HttpRequest, send::HttpSendOutput},
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::trace;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    rfc8620::{JmapSession, coroutine::JmapRedirectYield},
};

/// Failure causes during the JMAP session-get flow.
#[derive(Debug, Error)]
pub enum JmapSessionGetError {
    #[error("JMAP session-get failed: HTTP {0}")]
    HttpStatus(u16),
    #[error("JMAP session-get failed: no primary account for the mail capability")]
    NoPrimaryMailAccount,
    #[error("JMAP session-get failed: {0}")]
    Send(#[from] Http11SendError),
    #[error("JMAP session-get failed: parse session: {0}")]
    ParseSession(#[source] serde_json::Error),
}

/// Successful terminal output of [`JmapSessionGet`].
#[derive(Clone, Debug)]
pub struct JmapSessionGetOutput {
    pub session: JmapSession,
    pub keep_alive: bool,
}

/// I/O-free coroutine to fetch a JMAP session (RFC 8620 §2).
///
/// If `url` has a non-root path (e.g.
/// `https://api.fastmail.com/jmap/session/`), GETs that path directly
/// as the session endpoint. Otherwise GETs `/.well-known/jmap` for
/// automatic discovery.
///
/// When the server responds with a 3xx redirect, the coroutine yields
/// [`JmapRedirectYield::WantsRedirect`]. The caller is responsible for
/// opening a new connection and retrying with a new coroutine.
pub struct JmapSessionGet {
    state: State,
}

impl JmapSessionGet {
    /// `url` is either a base URL for discovery (`https://mail.example.com`,
    /// triggering `GET /.well-known/jmap`) or a direct session endpoint
    /// (`https://api.example.com/jmap/session/`, used as-is).
    pub fn new(http_auth: &SecretString, url: &Url) -> Self {
        let host = url.host_str().unwrap_or("localhost");

        let session_url = match url.path() {
            "" | "/" => {
                let mut u = url.clone();
                u.set_path("/.well-known/jmap");
                u
            }
            _ => url.clone(),
        };

        trace!("fetch JMAP session from {session_url}");

        let http_request = HttpRequest::get(session_url)
            .header("Host", host)
            .header("Accept", "application/json")
            .header("Authorization", http_auth.expose_secret());

        Self {
            state: State::Send(Http11Send::new(http_request)),
        }
    }
}

impl JmapCoroutine for JmapSessionGet {
    type Yield = JmapRedirectYield;
    type Return = Result<JmapSessionGetOutput, JmapSessionGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        trace!("session-get: {}", self.state);
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
                        let err = JmapSessionGetError::HttpStatus(*response.status);
                        return JmapCoroutineState::Complete(Err(err));
                    }

                    match serde_json::from_slice::<JmapSession>(&response.body) {
                        Ok(session) => JmapCoroutineState::Complete(Ok(JmapSessionGetOutput {
                            session,
                            keep_alive,
                        })),
                        Err(err) => JmapCoroutineState::Complete(Err(
                            JmapSessionGetError::ParseSession(err),
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

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{format, vec::Vec};

    use super::*;

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_url() -> Url {
        "https://api.example.com/".parse().unwrap()
    }

    fn session_json() -> &'static [u8] {
        br#"{
            "capabilities": {},
            "accounts": {},
            "primaryAccounts": {},
            "username": "alice",
            "apiUrl": "https://api.example.com/jmap/",
            "downloadUrl": "https://api.example.com/jmap/download/{accountId}/{blobId}/{name}?accept={type}",
            "uploadUrl": "https://api.example.com/jmap/upload/{accountId}/",
            "eventSourceUrl": "https://api.example.com/jmap/eventsource/?types={types}&closeafter={closeafter}&ping={ping}",
            "state": "abc"
        }"#
    }

    #[test]
    fn success_returns_ok() {
        let mut cor = JmapSessionGet::new(&make_auth(), &make_url());

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = session_json();
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
            body.len()
        );
        let mut bytes = reply.into_bytes();
        bytes.extend_from_slice(body);
        let out = expect_complete_ok(&mut cor, &bytes);
        assert_eq!(out.session.username, "alice");
    }

    #[test]
    fn http_error_returns_status() {
        let mut cor = JmapSessionGet::new(&make_auth(), &make_url());

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
        let err = expect_complete_err(&mut cor, reply);
        assert!(matches!(err, JmapSessionGetError::HttpStatus(401)));
    }

    #[test]
    fn redirect_yields_redirect() {
        let mut cor = JmapSessionGet::new(&make_auth(), &make_url());

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 301 Moved Permanently\r\nLocation: https://api2.example.com/.well-known/jmap\r\nContent-Length: 0\r\n\r\n";
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Yielded(JmapRedirectYield::WantsRedirect { url, .. }) => {
                assert_eq!(url.host_str(), Some("api2.example.com"));
            }
            state => panic!("expected WantsRedirect, got {state:?}"),
        }
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let mut cor = JmapSessionGet::new(&make_auth(), &make_url());

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = b"{not json";
        let reply = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
        let mut bytes = reply.into_bytes();
        bytes.extend_from_slice(body);
        let err = expect_complete_err(&mut cor, &bytes);
        assert!(matches!(err, JmapSessionGetError::ParseSession(_)));
    }

    #[test]
    fn uses_well_known_path_for_base_url() {
        let mut cor = JmapSessionGet::new(&make_auth(), &make_url());
        let bytes = expect_wants_write(&mut cor, None);
        let req = core::str::from_utf8(&bytes).expect("utf8 request");
        assert!(req.contains("/.well-known/jmap"));
    }

    // --- utils

    fn expect_wants_write(cor: &mut JmapSessionGet, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapRedirectYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapSessionGet) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapRedirectYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut JmapSessionGet, reply: &[u8]) -> JmapSessionGetOutput {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(cor: &mut JmapSessionGet, reply: &[u8]) -> JmapSessionGetError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
