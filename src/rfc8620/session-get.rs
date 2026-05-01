//! I/O-free coroutine to discover a JMAP session (RFC 8620 §2).

use alloc::vec::Vec;

use io_http::{
    rfc9110::request::HttpRequest,
    rfc9112::send::{Http11Send, Http11SendError, Http11SendResult},
};
use log::trace;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use url::Url;

use crate::rfc8620::session::JmapSession;

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapSessionGetError {
    #[error("Send HTTP GET /.well-known/jmap error: {0}")]
    SendHttp(#[from] Http11SendError),
    #[error("HTTP error status {0}")]
    HttpStatus(u16),
    #[error("Parse JMAP session object error: {0}")]
    ParseSession(#[source] serde_json::Error),
    #[error("No primary account found for the mail capability")]
    NoPrimaryMailAccount,
}

/// Result returned by the [`JmapSessionGet`] coroutine.
#[derive(Debug)]
pub enum JmapSessionGetResult {
    /// The coroutine successfully discovered the JMAP session.
    Ok {
        session: JmapSession,
        keep_alive: bool,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The server responded with a redirect to a new URL.
    ///
    /// The caller must open a new connection to the redirected URL and
    /// create a new [`JmapSessionGet`] coroutine targeting it.
    WantsRedirect {
        url: Url,
        keep_alive: bool,
        same_origin: bool,
    },
    /// The coroutine encountered an error.
    Err(JmapSessionGetError),
}

/// I/O-free coroutine to fetch a JMAP session (RFC 8620 §2).
///
/// If `url` has a non-root path (e.g. `https://api.fastmail.com/jmap/session/`),
/// GETs that path directly as the session endpoint. Otherwise GETs
/// `/.well-known/jmap` for automatic discovery.
///
/// When the server responds with a 3xx redirect, the coroutine returns
/// [`JmapSessionGetResult::WantsRedirect`]. The caller is responsible
/// for opening a new connection and retrying with a new coroutine.
pub struct JmapSessionGet {
    send: Http11Send,
}

impl JmapSessionGet {
    /// Creates a new session coroutine.
    ///
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
            send: Http11Send::new(http_request),
        }
    }

    /// Advances the coroutine.
    ///
    /// Pass [`None`] when there is no data to provide (initial call,
    /// after a write). Pass `Some(data)` with bytes read from the
    /// socket after a [`JmapSessionGetResult::WantsRead`]. Pass
    /// `Some(&[])` to signal EOF.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> JmapSessionGetResult {
        match self.send.resume(arg) {
            Http11SendResult::Ok {
                response,
                keep_alive,
                ..
            } => {
                if !response.status.is_success() {
                    let err = JmapSessionGetError::HttpStatus(*response.status);
                    return JmapSessionGetResult::Err(err);
                }

                match serde_json::from_slice::<JmapSession>(&response.body) {
                    Ok(session) => JmapSessionGetResult::Ok {
                        session,
                        keep_alive,
                    },
                    Err(err) => JmapSessionGetResult::Err(JmapSessionGetError::ParseSession(err)),
                }
            }
            Http11SendResult::WantsRead => JmapSessionGetResult::WantsRead,
            Http11SendResult::WantsWrite(bytes) => JmapSessionGetResult::WantsWrite(bytes),
            Http11SendResult::WantsRedirect {
                url,
                keep_alive,
                same_origin,
                ..
            } => JmapSessionGetResult::WantsRedirect {
                url,
                keep_alive,
                same_origin,
            },
            Http11SendResult::Err(err) => JmapSessionGetResult::Err(err.into()),
        }
    }
}
