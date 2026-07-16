//! Generic JMAP `Foo/get` coroutine (RFC 8620 §5.1): wraps [`JmapSend`] with a
//! single method-call batch and a typed response decoder.
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
//!     rfc8620::get::{JmapGet, JmapGetOptions},
//! };
//! use secrecy::SecretString;
//! use serde::Deserialize;
//! use url::Url;
//!
//! #[derive(Deserialize)]
//! struct Mailbox { id: String, name: String }
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let api_url: Url = "https://api.example.com/jmap/".parse().unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let mut coroutine = JmapGet::<Mailbox>::new(
//!     "a1".into(),
//!     &auth,
//!     &api_url,
//!     "Mailbox/get",
//!     vec!["urn:ietf:params:jmap:mail".into()],
//!     JmapGetOptions::default(),
//! )
//! .unwrap();
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
//! println!("got {} items", out.list.len());
//! ```

use core::marker::PhantomData;

use alloc::{string::String, vec::Vec};

use secrecy::SecretString;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    jmap_try,
    rfc8620::{error::JmapMethodError, request::JmapBatch, send::*},
};

/// Failure causes during a JMAP `Foo/get` flow.
#[derive(Debug, Error)]
pub enum JmapGetError {
    /// The response carried no method response.
    #[error("JMAP Foo/get failed: missing response in method_responses")]
    MissingResponse,
    /// The inner send coroutine failed.
    #[error("JMAP Foo/get failed: {0}")]
    Send(#[from] JmapSendError),
    /// The method arguments could not be serialized.
    #[error("JMAP Foo/get failed: serialize args: {0}")]
    SerializeArgs(#[source] serde_json::Error),
    /// The method response could not be parsed.
    #[error("JMAP Foo/get failed: parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
    /// The server returned a method-level error.
    #[error("JMAP Foo/get failed: {0}")]
    Method(#[from] JmapMethodError),
}

/// Options for [`JmapGet::new`].
#[derive(Clone, Debug, Default)]
pub struct JmapGetOptions {
    /// Restrict the fetch to these ids; `None` fetches all.
    pub ids: Option<Vec<String>>,
    /// Restrict the returned properties; `None` returns all.
    pub properties: Option<Vec<String>>,
}

/// Successful terminal output of the [`JmapGet`] coroutine.
#[derive(Clone, Debug)]
pub struct JmapGetOutput<T> {
    /// The fetched objects.
    pub list: Vec<T>,
    /// The requested ids the server did not find.
    pub not_found: Vec<String>,
    /// The server state the objects were fetched at.
    pub state: String,
    /// Whether the server indicated the connection can be reused.
    pub keep_alive: bool,
}

/// Generic I/O-free coroutine for the JMAP `Foo/get` method (RFC 8620 §5.1).
pub struct JmapGet<T> {
    state: State,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> JmapGet<T> {
    /// Builds a single-call `Foo/get` batch and wraps it in [`JmapSend`].
    pub fn new(
        account_id: String,
        http_auth: &SecretString,
        api_url: &Url,
        method: impl Into<String>,
        capabilities: Vec<String>,
        opts: JmapGetOptions,
    ) -> Result<Self, JmapGetError> {
        let args = serde_json::to_value(GetArgs {
            account_id: &account_id,
            ids: opts.ids.as_deref(),
            properties: opts.properties.as_deref(),
        })
        .map_err(JmapGetError::SerializeArgs)?;

        let mut batch = JmapBatch::new();
        batch.add(method, args);

        let request = batch.into_request(capabilities);

        Ok(Self {
            state: State::Send(JmapSend::new(http_auth, api_url, request)?),
            _phantom: PhantomData,
        })
    }

    /// Wraps a pre-built [`JmapSend`] (advanced: lets callers compose
    /// custom batches and still benefit from the typed response decode).
    pub fn from_send(send: JmapSend) -> Self {
        Self {
            state: State::Send(send),
            _phantom: PhantomData,
        }
    }
}

impl<T: DeserializeOwned> JmapCoroutine for JmapGet<T> {
    type Yield = JmapYield;
    type Return = Result<JmapGetOutput<T>, JmapGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match &mut self.state {
            State::Send(send) => {
                let JmapSendOutput {
                    response,
                    keep_alive,
                } = jmap_try!(send, arg);

                let Some((name, args, _)) = response.method_responses.into_iter().next() else {
                    return JmapCoroutineState::Complete(Err(JmapGetError::MissingResponse));
                };

                if name == "error" {
                    let err = serde_json::from_value::<JmapMethodError>(args)
                        .unwrap_or(JmapMethodError::Unknown);
                    return JmapCoroutineState::Complete(Err(err.into()));
                }

                match serde_json::from_value::<GetResponse<T>>(args) {
                    Ok(r) => JmapCoroutineState::Complete(Ok(JmapGetOutput {
                        list: r.list,
                        not_found: r.not_found,
                        state: r.state,
                        keep_alive,
                    })),
                    Err(err) => JmapCoroutineState::Complete(Err(JmapGetError::ParseResponse(err))),
                }
            }
        }
    }
}

enum State {
    Send(JmapSend),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetArgs<'a> {
    account_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ids: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<&'a [String]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetResponse<T> {
    list: Vec<T>,
    #[serde(default)]
    not_found: Vec<String>,
    state: String,
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString, vec};

    use crate::rfc8620::get::*;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Probe {
        id: String,
    }

    fn make_auth() -> SecretString {
        SecretString::from("Bearer test")
    }

    fn make_url() -> Url {
        "https://api.example.com/jmap/".parse().unwrap()
    }

    fn make_get() -> JmapGet<Probe> {
        JmapGet::<Probe>::new(
            "a1".to_string(),
            &make_auth(),
            &make_url(),
            "Mailbox/get",
            vec!["urn:ietf:params:jmap:mail".to_string()],
            JmapGetOptions::default(),
        )
        .unwrap()
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
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Mailbox/get", {"list":[{"id":"m1"}],"notFound":[],"state":"s1"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(200, body);
        let out = expect_complete_ok(&mut cor, &reply);
        assert_eq!(out.list, vec![Probe { id: "m1".into() }]);
        assert_eq!(out.state, "s1");
    }

    #[test]
    fn method_error_returns_method_error() {
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["error", {"type":"accountNotFound"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(200, body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapGetError::Method(_)));
    }

    #[test]
    fn missing_response_returns_missing_response() {
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{"methodResponses": [], "sessionState": "s1"}"#;
        let reply = build_http_reply(200, body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapGetError::MissingResponse));
    }

    #[test]
    fn parse_error_returns_parse_response() {
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let body = br#"{
            "methodResponses": [["Mailbox/get", {"list":"nope"}, "c0"]],
            "sessionState": "s1"
        }"#;
        let reply = build_http_reply(200, body);
        let err = expect_complete_err(&mut cor, &reply);
        assert!(matches!(err, JmapGetError::ParseResponse(_)));
    }

    #[test]
    fn http_error_surfaces_as_send_error() {
        let mut cor = make_get();
        expect_wants_write(&mut cor, None);
        expect_wants_read(&mut cor);

        let reply = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
        let err = expect_complete_err(&mut cor, reply);
        assert!(matches!(
            err,
            JmapGetError::Send(JmapSendError::HttpStatus(401))
        ));
    }

    fn expect_wants_write(cor: &mut JmapGet<Probe>, arg: Option<&[u8]>) -> Vec<u8> {
        match cor.resume(arg) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => bytes,
            state => panic!("expected WantsWrite, got {state:?}"),
        }
    }

    fn expect_wants_read(cor: &mut JmapGet<Probe>) {
        match cor.resume(None) {
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => {}
            state => panic!("expected WantsRead, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut JmapGet<Probe>, reply: &[u8]) -> JmapGetOutput<Probe> {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(cor: &mut JmapGet<Probe>, reply: &[u8]) -> JmapGetError {
        match cor.resume(Some(reply)) {
            JmapCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
