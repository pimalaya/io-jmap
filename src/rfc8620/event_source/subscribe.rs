//! I/O-free streaming coroutine that subscribes to a JMAP Event Source channel
//! (RFC 8620 §7.3) and yields one [`JmapStateChange`] per push frame.
//!
//! Composes [`Http11HeadersRead`] + [`Http11ChunksReadStream`] +
//! [`SseFrameParser`] + [`JmapStateChange::parse`] into one state machine.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     io::{Read, Write},
//!     net::TcpStream,
//!     sync::{Arc, atomic::AtomicBool},
//! };
//!
//! use io_jmap::{
//!     coroutine::{JmapCoroutine, JmapCoroutineState},
//!     rfc8620::{
//!         JmapSession,
//!         event_source::{
//!             JmapCloseAfter,
//!             subscribe::{JmapEventSource, JmapEventSourceYield},
//!         },
//!     },
//! };
//! use secrecy::SecretString;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("api.example.com:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let session: JmapSession = serde_json::from_str(r#"{
//!     "username": "",
//!     "accounts": {},
//!     "primaryAccounts": {"urn:ietf:params:jmap:mail": "a1"},
//!     "capabilities": {},
//!     "apiUrl": "https://api.example.com/jmap/",
//!     "downloadUrl": "",
//!     "uploadUrl": "",
//!     "eventSourceUrl": "https://api.example.com/jmap/eventsource/",
//!     "state": ""
//! }"#).unwrap();
//! let auth = SecretString::from("Bearer xyz");
//! let shutdown = Arc::new(AtomicBool::new(false));
//! let mut coroutine =
//!     JmapEventSource::new(&session, &auth, &["Email"], 30, JmapCloseAfter::State, shutdown)
//!         .unwrap();
//! let mut arg = None;
//!
//! loop {
//!     match coroutine.resume(arg.take()) {
//!         JmapCoroutineState::Yielded(JmapEventSourceYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         JmapCoroutineState::Yielded(JmapEventSourceYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         JmapCoroutineState::Yielded(JmapEventSourceYield::Frame(change)) => {
//!             println!("{change:?}");
//!         }
//!         JmapCoroutineState::Complete(Ok(())) => break,
//!         JmapCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! }
//! ```

use core::{
    mem,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::{format, string::String, sync::Arc, vec::Vec};

use io_http::{
    coroutine::*,
    rfc9110::{headers::HTTP_TRANSFER_ENCODING, request::HttpRequest},
    rfc9112::{
        chunk_stream::{
            Http11ChunksReadStream, Http11ChunksReadStreamError, Http11ChunksReadStreamYield,
        },
        read_headers::{Http11HeadersRead, Http11HeadersReadError, Http11HeadersReadOutput},
    },
    sse::frame::{SseFrameParser, SseFrameParserYield},
};
use log::{debug, trace};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    rfc8620::{
        JmapSession,
        event_source::{JmapCloseAfter, JmapStateChange, JmapStateChangeParseError},
    },
};

/// Per-step yield for [`JmapEventSource`].
#[derive(Debug)]
pub enum JmapEventSourceYield {
    /// One decoded push notification. Empty-data SSE frames (pings) surface as
    /// the default [`JmapStateChange`] with an empty `changed` map: keep-alive.
    Frame(JmapStateChange),
    /// The caller reads more bytes and feeds them back on the next resume.
    WantsRead,
    /// The caller writes these bytes; the next resume typically takes `None`.
    WantsWrite(Vec<u8>),
}

/// Failure causes during the JMAP event-source flow.
#[derive(Debug, Error)]
pub enum JmapEventSourceError {
    /// The server answered the subscription with a non-2xx status.
    #[error("JMAP event-source failed: HTTP {0}")]
    HttpStatus(u16),
    /// The streaming response did not use chunked transfer coding.
    #[error("JMAP event-source failed: response must be Transfer-Encoding: chunked")]
    NotChunked,
    /// The subscription URL built from the session could not be parsed.
    #[error("JMAP event-source failed: invalid URL {0}")]
    InvalidUrl(String),
    /// The response head could not be read.
    #[error("JMAP event-source failed: {0}")]
    ReadHeaders(#[from] Http11HeadersReadError),
    /// The chunked-body decoder failed.
    #[error("JMAP event-source failed: {0}")]
    ReadChunks(#[from] Http11ChunksReadStreamError),
    /// An SSE frame could not be decoded as a StateChange.
    #[error("JMAP event-source failed: {0}")]
    DecodeFrame(#[from] JmapStateChangeParseError),
}

/// I/O-free streaming coroutine for the JMAP `EventSource` push channel.
///
/// Cooperative shutdown: the coroutine polls the shared [`AtomicBool`] at the
/// top of each [`Self::resume`] and terminates with `Complete(Ok(()))` when
/// set. The caller's resume loop must honour the flag too to interrupt a
/// blocking socket read in flight.
pub struct JmapEventSource {
    state: State,
    shutdown: Arc<AtomicBool>,
}

impl JmapEventSource {
    /// Builds the JMAP push subscription URL: `event_source_url` plus
    /// `types=<csv>`, `closeafter=<v>` (see [`JmapCloseAfter`]) and
    /// `ping=<seconds>`. `types` may be empty for "all types".
    pub fn subscribe_url(
        session: &JmapSession,
        types: &[&str],
        ping_seconds: u64,
        close_after: JmapCloseAfter,
    ) -> String {
        let base = &session.event_source_url;
        let types = types.join(",");
        let sep = if base.contains('?') { '&' } else { '?' };
        let close_after = close_after.as_str();
        format!("{base}{sep}types={types}&closeafter={close_after}&ping={ping_seconds}")
    }

    /// Builds the subscription URL from the session and prepares the initial
    /// `GET` request bytes.
    ///
    /// `types` filters JMAP data types (empty = all). `ping_seconds` sets the
    /// server heartbeat cadence. `close_after` picks the lifecycle (see
    /// [`JmapCloseAfter`]). Flip `shutdown` to wind the coroutine down.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        types: &[&str],
        ping_seconds: u64,
        close_after: JmapCloseAfter,
        shutdown: Arc<AtomicBool>,
    ) -> Result<Self, JmapEventSourceError> {
        let url_str = Self::subscribe_url(session, types, ping_seconds, close_after);
        let url = Url::parse(&url_str).map_err(|_| JmapEventSourceError::InvalidUrl(url_str))?;

        let host = url.host_str().unwrap_or("localhost");
        let request = HttpRequest::get(url.clone())
            .header("Host", host)
            .header("Accept", "text/event-stream")
            .header("Authorization", http_auth.expose_secret());

        debug!("prepare event source subscription request");
        trace!("subscription url: {url}");

        Ok(Self {
            state: State::SendingRequest(request.to_http_11_vec()),
            shutdown,
        })
    }
}

impl JmapCoroutine for JmapEventSource {
    type Yield = JmapEventSourceYield;
    type Return = Result<(), JmapEventSourceError>;

    /// Advances the coroutine.
    ///
    /// `None` on the initial call; `Some(data)` after a
    /// [`JmapEventSourceYield::WantsRead`]. `Some(&[])` is EOF: it's an error
    /// during the head stage, a clean `Complete(Ok(()))` during streaming.
    fn resume(&mut self, mut arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        if self.shutdown.load(Ordering::SeqCst) {
            self.state = State::Done;
            return JmapCoroutineState::Complete(Ok(()));
        }

        loop {
            match &mut self.state {
                State::SendingRequest(_) => {
                    let State::SendingRequest(bytes) = mem::replace(
                        &mut self.state,
                        State::ReadingHead(Http11HeadersRead::default()),
                    ) else {
                        unreachable!()
                    };
                    return JmapCoroutineState::Yielded(JmapEventSourceYield::WantsWrite(bytes));
                }
                State::ReadingHead(head) => match head.resume(arg.take()) {
                    HttpCoroutineState::Yielded(HttpYield::WantsRead) => {
                        return JmapCoroutineState::Yielded(JmapEventSourceYield::WantsRead);
                    }
                    HttpCoroutineState::Yielded(HttpYield::WantsWrite(_)) => {
                        unreachable!("Http11HeadersRead never writes");
                    }
                    HttpCoroutineState::Complete(Err(err)) => {
                        return JmapCoroutineState::Complete(Err(err.into()));
                    }
                    HttpCoroutineState::Complete(Ok(Http11HeadersReadOutput {
                        response,
                        remaining,
                        keep_alive: _,
                    })) => {
                        if !response.status.is_success() {
                            return JmapCoroutineState::Complete(Err(
                                JmapEventSourceError::HttpStatus(*response.status),
                            ));
                        }
                        let chunked = response
                            .header(HTTP_TRANSFER_ENCODING)
                            .is_some_and(|enc| enc.eq_ignore_ascii_case("chunked"));
                        if !chunked {
                            return JmapCoroutineState::Complete(Err(
                                JmapEventSourceError::NotChunked,
                            ));
                        }
                        // NOTE: head reader may over-read past `\r\n\r\n`
                        // into the chunked body; prime the chunk decoder so
                        // the SSE parser never sees `<hex>\r\n` size headers.
                        let mut chunks = Http11ChunksReadStream::default();
                        let pending = if remaining.is_empty() {
                            None
                        } else {
                            match chunks.resume(Some(&remaining)) {
                                HttpCoroutineState::Yielded(
                                    Http11ChunksReadStreamYield::Frame { body },
                                ) => Some(body),
                                HttpCoroutineState::Yielded(
                                    Http11ChunksReadStreamYield::WantsRead,
                                ) => None,
                                HttpCoroutineState::Complete(Ok(_)) => {
                                    self.state = State::Done;
                                    return JmapCoroutineState::Complete(Ok(()));
                                }
                                HttpCoroutineState::Complete(Err(err)) => {
                                    return JmapCoroutineState::Complete(Err(err.into()));
                                }
                            }
                        };
                        self.state = State::Streaming {
                            chunks,
                            parser: SseFrameParser::default(),
                            pending,
                        };
                        // NOTE: fall into the streaming arm so the parser
                        // drains the primed buffer before asking for bytes.
                    }
                },
                State::Streaming {
                    chunks,
                    parser,
                    pending,
                } => {
                    let parser_input = pending.take();
                    match parser.resume(parser_input.as_deref()) {
                        HttpCoroutineState::Yielded(SseFrameParserYield::Frame(frame)) => {
                            return match JmapStateChange::parse(&frame.data) {
                                Ok(change) => {
                                    JmapCoroutineState::Yielded(JmapEventSourceYield::Frame(change))
                                }
                                Err(err) => JmapCoroutineState::Complete(Err(err.into())),
                            };
                        }
                        HttpCoroutineState::Yielded(SseFrameParserYield::WantsBytes) => {
                            match chunks.resume(arg.take()) {
                                HttpCoroutineState::Yielded(
                                    Http11ChunksReadStreamYield::Frame { body },
                                ) => {
                                    *pending = Some(body);
                                }
                                HttpCoroutineState::Complete(Ok(_)) => {
                                    self.state = State::Done;
                                    return JmapCoroutineState::Complete(Ok(()));
                                }
                                HttpCoroutineState::Yielded(
                                    Http11ChunksReadStreamYield::WantsRead,
                                ) => {
                                    return JmapCoroutineState::Yielded(
                                        JmapEventSourceYield::WantsRead,
                                    );
                                }
                                HttpCoroutineState::Complete(Err(err)) => {
                                    return JmapCoroutineState::Complete(Err(err.into()));
                                }
                            }
                        }
                        HttpCoroutineState::Complete(never) => match never {},
                    }
                }
                State::Done => return JmapCoroutineState::Complete(Ok(())),
            }
        }
    }
}

/// Internal progression state of [`JmapEventSource`].
enum State {
    /// Initial: yield the GET request bytes once, then transition to head.
    SendingRequest(Vec<u8>),
    /// Resuming [`Http11HeadersRead`] on the response.
    ReadingHead(Http11HeadersRead),
    /// Pumping chunks into the SSE parser, decoding each frame as a
    /// `JmapStateChange`.
    Streaming {
        chunks: Http11ChunksReadStream,
        parser: SseFrameParser,
        /// Decoded chunk body waiting to be fed to the SSE parser.
        pending: Option<Vec<u8>>,
    },
    /// Terminal: shutdown flipped, or stream finished.
    Done,
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::AtomicBool;

    use alloc::{
        collections::BTreeMap,
        format,
        string::{String, ToString},
        sync::Arc,
        vec::Vec,
    };

    use secrecy::SecretString;

    use crate::{
        coroutine::*,
        rfc8620::{JmapSession, event_source::subscribe::*, event_source::*},
    };

    fn synthetic_session() -> JmapSession {
        JmapSession {
            username: String::new(),
            accounts: BTreeMap::new(),
            primary_accounts: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            api_url: "https://example.org/api".parse().unwrap(),
            download_url: String::new(),
            upload_url: String::new(),
            event_source_url: String::new(),
            state: String::new(),
        }
    }

    #[test]
    fn subscribe_url_appends_query_params() {
        let session = JmapSession {
            event_source_url: "https://jmap.example.org/events".into(),
            ..synthetic_session()
        };
        let url = JmapEventSource::subscribe_url(
            &session,
            &["Email", "EmailDelivery"],
            30,
            JmapCloseAfter::No,
        );
        assert_eq!(
            url,
            "https://jmap.example.org/events?types=Email,EmailDelivery&closeafter=no&ping=30"
        );
    }

    #[test]
    fn subscribe_url_preserves_existing_query() {
        let session = JmapSession {
            event_source_url: "https://jmap.example.org/events?token=abc".into(),
            ..synthetic_session()
        };
        let url = JmapEventSource::subscribe_url(&session, &[], 15, JmapCloseAfter::State);
        assert_eq!(
            url,
            "https://jmap.example.org/events?token=abc&types=&closeafter=state&ping=15"
        );
    }

    // NOTE: regression guard. Head reader over-reads into the chunked body;
    // those leftover bytes must go through the chunk decoder, not straight
    // to the SSE parser. This body splits `data:` across two chunks so the
    // broken path produces invalid JSON.
    #[test]
    fn streaming_head_leftover_is_chunk_decoded() {
        let session = JmapSession {
            event_source_url: "https://example.org/sse".into(),
            ..synthetic_session()
        };
        let auth = SecretString::from("Bearer t".to_string());
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut es = JmapEventSource::new(
            &session,
            &auth,
            &["Email"],
            30,
            JmapCloseAfter::State,
            shutdown,
        )
        .unwrap();

        // NOTE: drain the initial GET-request write so the next resume
        // enters the ReadingHead arm.
        let JmapCoroutineState::Yielded(JmapEventSourceYield::WantsWrite(_)) = es.resume(None)
        else {
            panic!("expected initial WantsWrite");
        };

        let part1 = "event: state\ndata: {\"@type\":\"StateChange\",\"changed\":{\"u1\":";
        let part2 = "{\"Email\":\"s1\"}}}\n\n";
        let chunked = format!(
            "{:x}\r\n{part1}\r\n{:x}\r\n{part2}\r\n0\r\n\r\n",
            part1.len(),
            part2.len(),
        );
        let head = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/event-stream\r\n\r\n";
        let mut wire: Vec<u8> = head.as_bytes().to_vec();
        wire.extend_from_slice(chunked.as_bytes());

        // NOTE: head + body arrive in one socket read; matches the Fastmail
        // trace shape that triggered the bug.
        match es.resume(Some(&wire)) {
            JmapCoroutineState::Yielded(JmapEventSourceYield::Frame(change)) => {
                assert_eq!(change.r#type, "StateChange");
                assert_eq!(change.changed["u1"]["Email"], "s1");
            }
            other => panic!("expected Frame yield, got {other:?}"),
        }
    }
}
