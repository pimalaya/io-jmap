//! Base coroutine all higher-level JMAP coroutines delegate to: serialises a
//! [`JmapRequest`] as JSON, runs an HTTP/1.1 POST, and deserialises the
//! [`JmapResponse`] body.
//!
//! 3xx redirects surface as [`JmapSendError::UnexpectedRedirect`];
//! redirect-aware coroutines resume [`Http11Send`] directly instead.
//!
//! [`JmapRequest`]: crate::rfc8620::JmapRequest
//! [`JmapResponse`]: crate::rfc8620::JmapResponse
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
//!     coroutine::{JmapCoroutine, JmapCoroutineState, JmapYield},
//!     rfc8620::{JmapBatch, send::JmapSend},
//! };
//! use secrecy::SecretString;
//! use serde_json::json;
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let mut batch = JmapBatch::new();
//! batch.add("Email/get", json!({ "accountId": "a1", "ids": null }));
//! let request = batch.into_request(vec!["urn:ietf:params:jmap:core".into()]);
//!
//! let api_url: Url = "https://api.example.com/jmap/".parse().unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine = JmapSend::new(&auth, &api_url, request).unwrap();
//! let mut arg = None;
//!
//! let out = loop {
//!     match coroutine.resume(arg.take()) {
//!         JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         JmapCoroutineState::Yielded(JmapYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         JmapCoroutineState::Complete(Ok(out)) => break out,
//!         JmapCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{:?}", out.response);
//! ```

use io_http::{
    coroutine::*,
    rfc9110::{
        request::HttpRequest,
        send::{HttpSendOutput, HttpSendYield},
    },
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::{debug, trace};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    rfc8620::{JmapRequest, JmapResponse},
};

/// Failure causes during the JMAP send flow.
#[derive(Debug, Error)]
pub enum JmapSendError {
    /// The server answered with a non-2xx status.
    #[error("JMAP send failed: HTTP {0}")]
    HttpStatus(u16),
    /// The server answered with an unexpected redirect.
    #[error("JMAP send failed: unexpected redirect")]
    UnexpectedRedirect,
    /// The inner HTTP/1.1 send coroutine failed.
    #[error("JMAP send failed: {0}")]
    Send(#[from] Http11SendError),
    /// The request could not be serialized.
    #[error("JMAP send failed: serialize request: {0}")]
    SerializeRequest(#[source] serde_json::Error),
    /// The method response could not be parsed.
    #[error("JMAP send failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
}

/// Successful terminal output of the [`JmapSend`] coroutine.
#[derive(Clone, Debug)]
pub struct JmapSendOutput {
    /// The parsed JMAP response.
    pub response: JmapResponse,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// I/O-free coroutine sending one JMAP API request and parsing its response.
pub struct JmapSend {
    state: State,
}

impl JmapSend {
    /// Serialises `request` as JSON and builds an HTTP POST to `api_url`
    /// with the bearer token from `http_auth`.
    pub fn new(
        http_auth: &SecretString,
        api_url: &Url,
        request: JmapRequest,
    ) -> Result<Self, JmapSendError> {
        let body = serde_json::to_vec(&request).map_err(JmapSendError::SerializeRequest)?;

        let host = api_url.host_str().unwrap_or("localhost");

        let mut http_request = HttpRequest::get(api_url.clone())
            .header("Host", host)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("Authorization", http_auth.expose_secret())
            .body(body);

        http_request.method = "POST".into();

        debug!("prepare request to send");
        trace!("api url: {api_url}");

        Ok(Self {
            state: State::Send(Http11Send::new(http_request)),
        })
    }
}

impl JmapCoroutine for JmapSend {
    type Yield = JmapYield;
    type Return = Result<JmapSendOutput, JmapSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => match send.resume(arg) {
                HttpCoroutineState::Yielded(HttpSendYield::WantsRead) => {
                    JmapCoroutineState::Yielded(JmapYield::WantsRead)
                }
                HttpCoroutineState::Yielded(HttpSendYield::WantsWrite(bytes)) => {
                    JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes))
                }
                HttpCoroutineState::Yielded(HttpSendYield::WantsRedirect { .. }) => {
                    JmapCoroutineState::Complete(Err(JmapSendError::UnexpectedRedirect))
                }
                HttpCoroutineState::Complete(Err(err)) => {
                    JmapCoroutineState::Complete(Err(err.into()))
                }
                HttpCoroutineState::Complete(Ok(HttpSendOutput {
                    response,
                    keep_alive,
                    ..
                })) => {
                    if !response.status.is_success() {
                        let err = JmapSendError::HttpStatus(*response.status);
                        return JmapCoroutineState::Complete(Err(err));
                    }

                    match serde_json::from_slice::<JmapResponse>(&response.body) {
                        Ok(response) => JmapCoroutineState::Complete(Ok(JmapSendOutput {
                            response,
                            keep_alive,
                        })),
                        Err(err) => {
                            JmapCoroutineState::Complete(Err(JmapSendError::ParseResponse(err)))
                        }
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
    use alloc::{format, string::ToString, vec, vec::Vec};

    use crate::rfc8620::JmapBatch;
    use crate::rfc8620::send::*;

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_url() -> Url {
        "https://api.example.com/jmap/".parse().unwrap()
    }

    fn make_request() -> JmapRequest {
        let mut batch = JmapBatch::new();
        batch.add("Mailbox/get", serde_json::json!({ "accountId": "a1" }));
        batch.into_request(vec!["urn:ietf:params:jmap:core".to_string()])
    }

    fn make_response_body() -> Vec<u8> {
        br#"{
            "methodResponses": [["Mailbox/get", {"list":[],"notFound":[],"state":"s1"}, "c0"]],
            "sessionState": "s1"
        }"#
        .to_vec()
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
        let mut cor = JmapSend::new(&make_auth(), &make_url(), make_request()).unwrap();

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(200, &make_response_body());
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.response.method_responses.len(), 1);
        assert_eq!(out.response.session_state, "s1");
    }

    #[test]
    fn http_error_returns_status() {
        let mut cor = JmapSend::new(&make_auth(), &make_url(), make_request()).unwrap();

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
        let err = expect_complete_err(&mut cor, reply);
        assert!(matches!(err, JmapSendError::HttpStatus(401)));
    }

    #[test]
    fn redirect_returns_unexpected_redirect() {
        let mut cor = JmapSend::new(&make_auth(), &make_url(), make_request()).unwrap();

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 301 Moved\r\nLocation: https://other.example.com/jmap/\r\nContent-Length: 0\r\n\r\n";
        let err = expect_complete_err(&mut cor, reply);
        assert!(matches!(err, JmapSendError::UnexpectedRedirect));
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let mut cor = JmapSend::new(&make_auth(), &make_url(), make_request()).unwrap();

        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = build_http_reply(200, b"{not json");
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapSendError::ParseResponse(_)));
    }

    #[test]
    fn batch_assigns_sequential_ids() {
        let mut batch = JmapBatch::new();
        let a = batch.add("A", serde_json::json!({}));
        let b = batch.add("B", serde_json::json!({}));
        assert_eq!(a, "c0");
        assert_eq!(b, "c1");
    }

    fn expect_wants_write(cor: &mut JmapSend, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapSend) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut JmapSend, reply: &[u8]) -> JmapSendOutput {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(cor: &mut JmapSend, reply: &[u8]) -> JmapSendError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
