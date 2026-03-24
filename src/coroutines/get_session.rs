//! I/O-free coroutine to discover a JMAP session (RFC 8620 §2).

use http::{
    header::{ACCEPT, AUTHORIZATION},
    Method,
};
use io_http::v1_1::coroutines::{follow_redirects::*, send::*};
use io_stream::io::StreamIo;
use log::info;
use secrecy::ExposeSecret;
use thiserror::Error;
use url::Url;

use crate::{
    context::JmapContext,
    types::session::{capabilities, JmapSession},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum GetJmapSessionError {
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

/// Result returned by the [`GetJmapSession`] coroutine.
#[derive(Debug)]
pub enum GetJmapSessionResult {
    /// The coroutine successfully discovered the JMAP session.
    Ok {
        context: JmapContext,
        keep_alive: bool,
    },
    /// The coroutine wants stream I/O.
    Io(StreamIo),
    /// The coroutine encountered an error.
    Err(GetJmapSessionError),
    Reset(http::Uri),
}

/// I/O-free coroutine to discover a JMAP session.
///
/// Sends `GET /.well-known/jmap` (RFC 8620 §2) to discover the
/// server's JMAP session object, which contains the `apiUrl`,
/// account IDs, and capability configuration.
///
/// The discovered session is stored in the returned `context`.
pub struct GetJmapSession {
    context: Option<JmapContext>,
    send: FollowHttpRedirects,
}

impl GetJmapSession {
    /// Creates a new session discovery coroutine.
    ///
    /// `base_url` should be the HTTPS base URL of the server
    /// (e.g. `https://mail.example.com`). The `/.well-known/jmap`
    /// path is appended automatically.
    pub fn new(context: JmapContext, base_url: &Url) -> Result<Self, GetJmapSessionError> {
        let host = base_url.host_str().unwrap_or("localhost");

        let mut builder = http::Request::builder()
            .method(Method::GET)
            .uri("/.well-known/jmap")
            .header("Host", host)
            .header(ACCEPT, "application/json");

        if let Some(auth) = &context.http_auth {
            let auth = auth.expose_secret();
            builder = builder.header(AUTHORIZATION, auth);
        }

        let http_request = builder.body(vec![])?;

        info!("discover JMAP session at {base_url}/.well-known/jmap");

        Ok(Self {
            context: Some(context),
            send: FollowHttpRedirects::new(SendHttp::new(http_request)),
        })
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<StreamIo>) -> GetJmapSessionResult {
        let ok = match self.send.resume(arg) {
            FollowHttpRedirectsResult::Ok(ok) => ok,
            FollowHttpRedirectsResult::Io(io) => return GetJmapSessionResult::Io(io),
            FollowHttpRedirectsResult::Reset(uri) => {
                let host = uri.host().unwrap_or("localhost");

                let mut builder = http::Request::builder()
                    .method(Method::GET)
                    .uri(uri.clone())
                    .header("Host", host)
                    .header(ACCEPT, "application/json");

                if let Some(auth) = &self.context.as_ref().unwrap().http_auth {
                    let auth = auth.expose_secret();
                    builder = builder.header(AUTHORIZATION, auth);
                }

                let http_request = match builder.body(vec![]) {
                    Ok(req) => req,
                    Err(err) => return GetJmapSessionResult::Err(err.into()),
                };

                self.send = FollowHttpRedirects::new(SendHttp::new(http_request));
                return GetJmapSessionResult::Reset(uri);
            }
            FollowHttpRedirectsResult::Err(err) => return GetJmapSessionResult::Err(err.into()),
        };

        if !ok.response.status().is_success() {
            return GetJmapSessionResult::Err(GetJmapSessionError::HttpStatus(
                ok.response.status().as_u16(),
            ));
        }

        let session = match serde_json::from_slice::<JmapSession>(ok.response.body()) {
            Ok(s) => s,
            Err(err) => return GetJmapSessionResult::Err(GetJmapSessionError::ParseSession(err)),
        };

        let mut context = self.context.take().unwrap_or_default();

        // Derive the primary mail account ID from the session.
        let account_id = session.primary_accounts.get(capabilities::MAIL).cloned();

        context.account_id = account_id;
        context.session = Some(session);

        GetJmapSessionResult::Ok {
            context,
            keep_alive: ok.keep_alive,
        }
    }
}
