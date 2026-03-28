//! I/O-free coroutine to discover a JMAP session (RFC 8620 §2).

use http::{
    header::{ACCEPT, AUTHORIZATION},
    Method,
};
use io_http::v1_1::coroutines::{follow_redirects::*, send::*};
use io_stream::io::StreamIo;
use log::info;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use thiserror::Error;
use url::Url;

use crate::rfc8620::types::session::JmapSession;

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapSessionGetError {
    #[error("Build HTTP request error: {0}")]
    BuildHttp(#[from] http::Error),
    #[error("Send HTTP GET /.well-known/jmap error: {0}")]
    SendHttp(#[from] FollowHttpRedirectsError),
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
    /// The coroutine wants stream I/O.
    Io {
        io: StreamIo,
    },
    /// The coroutine encountered an error.
    Err {
        err: JmapSessionGetError,
    },
    Reset(http::Uri),
}

/// I/O-free coroutine to fetch a JMAP session (RFC 8620 §2).
///
/// If `url` has a non-root path (e.g. `https://api.fastmail.com/jmap/api/`),
/// GETs that path directly as the session endpoint. Otherwise GETs
/// `/.well-known/jmap` for automatic discovery.
pub struct JmapSessionGet {
    http_auth: SecretString,
    send: FollowHttpRedirects,
}

impl JmapSessionGet {
    /// Creates a new session coroutine.
    ///
    /// `url` is either a base URL for discovery (`https://mail.example.com`,
    /// triggering `GET /.well-known/jmap`) or a direct session endpoint
    /// (`https://api.example.com/jmap/session/`, used as-is).
    pub fn new(http_auth: &SecretString, url: &Url) -> Result<Self, JmapSessionGetError> {
        let host = url.host_str().unwrap_or("localhost");

        let path = match url.path() {
            "" | "/" => "/.well-known/jmap",
            p => p,
        };

        let http_request = http::Request::builder()
            .method(Method::GET)
            .uri(path)
            .header("Host", host)
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, http_auth.expose_secret())
            .body(vec![])?;

        info!("fetch JMAP session from {host}{path}");

        Ok(Self {
            http_auth: http_auth.clone(),
            send: FollowHttpRedirects::new(SendHttp::new(http_request)),
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> JmapSessionGetResult {
        let ok = match self.send.resume(arg) {
            FollowHttpRedirectsResult::Ok(ok) => ok,
            FollowHttpRedirectsResult::Io(io) => return JmapSessionGetResult::Io { io },
            FollowHttpRedirectsResult::Reset(uri) => {
                let host = uri.host().unwrap_or("localhost");

                let mut builder = http::Request::builder()
                    .method(Method::GET)
                    .uri(uri.clone())
                    .header("Host", host)
                    .header(ACCEPT, "application/json");

                builder = builder.header(AUTHORIZATION, self.http_auth.expose_secret());

                let http_request = match builder.body(vec![]) {
                    Ok(req) => req,
                    Err(err) => return JmapSessionGetResult::Err { err: err.into() },
                };

                self.send = FollowHttpRedirects::new(SendHttp::new(http_request));
                return JmapSessionGetResult::Reset(uri);
            }
            FollowHttpRedirectsResult::Err(err) => {
                return JmapSessionGetResult::Err { err: err.into() }
            }
        };

        if !ok.response.status().is_success() {
            return JmapSessionGetResult::Err {
                err: JmapSessionGetError::HttpStatus(ok.response.status().as_u16()),
            };
        }

        let session = match serde_json::from_slice::<JmapSession>(ok.response.body()) {
            Ok(s) => s,
            Err(err) => {
                return JmapSessionGetResult::Err {
                    err: JmapSessionGetError::ParseSession(err),
                }
            }
        };

        JmapSessionGetResult::Ok {
            session,
            keep_alive: ok.keep_alive,
        }
    }
}
