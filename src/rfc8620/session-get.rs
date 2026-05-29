//! I/O-free coroutine to discover a JMAP session (RFC 8620 §2).

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
use crate::rfc8620::{redirect::JmapRedirectYield, session::JmapSession};

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

/// Successful terminal output of [`JmapSessionGet`].
#[derive(Clone, Debug)]
pub struct JmapSessionGetOutput {
    pub session: JmapSession,
    pub keep_alive: bool,
}

/// I/O-free coroutine to fetch a JMAP session (RFC 8620 §2).
///
/// If `url` has a non-root path (e.g. `https://api.fastmail.com/jmap/session/`),
/// GETs that path directly as the session endpoint. Otherwise GETs
/// `/.well-known/jmap` for automatic discovery.
///
/// When the server responds with a 3xx redirect, the coroutine yields
/// [`JmapRedirectYield::WantsRedirect`]. The caller is responsible for
/// opening a new connection and retrying with a new coroutine.
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
}

impl JmapCoroutine for JmapSessionGet {
    type Yield = JmapRedirectYield;
    type Return = Result<JmapSessionGetOutput, JmapSessionGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.send.resume(arg) {
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
                    Err(err) => {
                        JmapCoroutineState::Complete(Err(JmapSessionGetError::ParseSession(err)))
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
            }) => JmapCoroutineState::Yielded(JmapRedirectYield::WantsRedirect {
                url,
                keep_alive,
                same_origin,
            }),
            HttpCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
        }
    }
}
