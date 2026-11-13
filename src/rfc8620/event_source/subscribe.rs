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
//!         session::JmapSession,
//!         event_source::{
//!             JmapCloseAfter,
//!             subscribe::{
//!                 JmapEventSource, JmapEventSourceOptions,
//!                 JmapEventSourceYield,
//!             },
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
//! let mut coroutine = JmapEventSource::new(
//!     &session,
//!     &auth,
//!     &["Email"],
//!     30,
//!     JmapCloseAfter::State,
//!     shutdown,
//!     JmapEventSourceOptions::default(),
//! )
//! .unwrap();
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
//!         JmapCoroutineState::Yielded(JmapEventSourceYield::Frame {
//!             change,
//!             ..
//!         }) => {
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
        event_source::{JmapCloseAfter, JmapStateChange, JmapStateChangeParseError},
        session::JmapSession,
    },
};

/// Default value of [`JmapEventSource::max_frame_len`]: 64 KiB.
pub const JMAP_DEFAULT_MAX_FRAME_LEN: usize = 64 * 1024;

/// Default value of [`JmapEventSource::max_chunk_len`]: 1 MiB.
pub const JMAP_DEFAULT_MAX_CHUNK_LEN: usize = 1024 * 1024;

/// Per-step yield for [`JmapEventSource`].
#[derive(Debug)]
pub enum JmapEventSourceYield {
    /// One decoded push notification, from a frame naming the `state` event
    /// or none. A frame with no data surfaces as the default
    /// [`JmapStateChange`] with an empty `changed` map: keep-alive.
    Frame {
        /// The decoded StateChange.
        change: JmapStateChange,
        /// The last event id the server set, to hand back as
        /// [`JmapEventSourceOptions::last_event_id`] when reconnecting.
        id: Option<String>,
    },
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
    /// The last event id holds a CR, LF or NUL (RFC 9110 §5.5).
    #[error("JMAP event-source failed: invalid last event id")]
    InvalidLastEventId,
    /// The response head could not be read.
    #[error("JMAP event-source failed: {0}")]
    ReadHeaders(#[from] Http11HeadersReadError),
    /// The chunked-body decoder failed.
    #[error("JMAP event-source failed: {0}")]
    ReadChunks(#[from] Http11ChunksReadStreamError),
    /// An SSE frame exceeds [`JmapEventSource::max_frame_len`].
    #[error("JMAP event-source failed: frame exceeds {0} bytes")]
    FrameTooLarge(usize),
    /// An SSE frame could not be decoded as a StateChange.
    #[error("JMAP event-source failed: {0}")]
    DecodeFrame(#[from] JmapStateChangeParseError),
}

/// Options for [`JmapEventSource::new`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JmapEventSourceOptions {
    /// The id of the last event seen on a previous connection, sent as
    /// `Last-Event-ID` for the server to push what changed since.
    pub last_event_id: Option<String>,
}

/// I/O-free streaming coroutine for the JMAP `EventSource` push channel.
///
/// Cooperative shutdown: the coroutine polls the shared [`AtomicBool`] at the
/// top of each [`Self::resume`] and terminates with `Complete(Ok(()))` when
/// set. The caller's resume loop must honour the flag too to interrupt a
/// blocking socket read in flight.
pub struct JmapEventSource {
    /// Maximum size in bytes of one SSE frame, comment lines aside, past
    /// which the stream fails with [`JmapEventSourceError::FrameTooLarge`].
    /// Defaults to [`JMAP_DEFAULT_MAX_FRAME_LEN`].
    pub max_frame_len: usize,
    /// Maximum size in bytes of one chunk, however many frames it holds,
    /// past which the stream fails with a `ChunkTooLarge`. Defaults to
    /// [`JMAP_DEFAULT_MAX_CHUNK_LEN`].
    pub max_chunk_len: usize,
    state: State,
    frame: FrameLen,
    shutdown: Arc<AtomicBool>,
}

impl JmapEventSource {
    /// Builds the JMAP push subscription URL from `event_source_url`, with
    /// `types` (comma-separated, `*` when empty), `closeafter` (see
    /// [`JmapCloseAfter`]) and `ping` (seconds).
    ///
    /// RFC 8620 §7.3 makes `event_source_url` a URI template naming the
    /// three variables, which servers send as
    /// `…?types={types}&closeafter={closeafter}&ping={ping}`, so they are
    /// expanded in place. A URL naming none of them gets them appended.
    pub fn subscribe_url(
        session: &JmapSession,
        types: &[&str],
        ping_seconds: u64,
        close_after: JmapCloseAfter,
    ) -> String {
        let base = &session.event_source_url;
        let types = match types {
            [] => "*".into(),
            types => types.join(","),
        };
        let close_after = close_after.as_str();
        let ping = format!("{ping_seconds}");

        let templated = ["{types}", "{closeafter}", "{ping}"]
            .iter()
            .any(|variable| base.contains(variable));

        if templated {
            return base
                .replace("{types}", &types)
                .replace("{closeafter}", close_after)
                .replace("{ping}", &ping);
        }

        let sep = if base.contains('?') { '&' } else { '?' };
        format!("{base}{sep}types={types}&closeafter={close_after}&ping={ping}")
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
        opts: JmapEventSourceOptions,
    ) -> Result<Self, JmapEventSourceError> {
        let url_str = Self::subscribe_url(session, types, ping_seconds, close_after);
        let url = Url::parse(&url_str).map_err(|_| JmapEventSourceError::InvalidUrl(url_str))?;

        let mut request = HttpRequest::get(url.clone())
            .header("Accept", "text/event-stream")
            .header("Authorization", http_auth.expose_secret());

        if let Some(id) = opts.last_event_id {
            if id.bytes().any(|b| matches!(b, b'\r' | b'\n' | 0)) {
                return Err(JmapEventSourceError::InvalidLastEventId);
            }

            request = request.header("Last-Event-ID", id);
        }

        debug!("prepare event source subscription request");
        trace!(
            "subscription url: {}://{}{}",
            url.scheme(),
            url.host_str().unwrap_or_default(),
            url.path(),
        );

        Ok(Self {
            max_frame_len: JMAP_DEFAULT_MAX_FRAME_LEN,
            max_chunk_len: JMAP_DEFAULT_MAX_CHUNK_LEN,
            state: State::SendingRequest(request.to_http_11_vec()),
            frame: FrameLen::default(),
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
                        chunks.max_chunk_len = self.max_chunk_len;

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

                    if let Some(bytes) = &parser_input
                        && !self.frame.count(bytes, self.max_frame_len)
                    {
                        let err = JmapEventSourceError::FrameTooLarge(self.max_frame_len);
                        self.state = State::Done;
                        return JmapCoroutineState::Complete(Err(err));
                    }

                    match parser.resume(parser_input.as_deref()) {
                        HttpCoroutineState::Yielded(SseFrameParserYield::Frame(frame)) => {
                            if let Some(event) = &frame.event
                                && event != "state"
                            {
                                trace!("ignore event `{event}`");
                                continue;
                            }

                            return match JmapStateChange::parse(&frame.data) {
                                Ok(change) => {
                                    let id = frame.id;
                                    let frame = JmapEventSourceYield::Frame { change, id };
                                    JmapCoroutineState::Yielded(frame)
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

/// Size of the SSE frame in progress: the lines since the last blank
/// line, comment lines and line ends aside.
#[derive(Default)]
struct FrameLen {
    /// Bytes of the complete field lines.
    fields: usize,
    /// Bytes of the line in progress.
    line: usize,
    /// Whether the line in progress is a comment.
    comment: bool,
    /// Whether the last byte was a CR, whose LF ends no line.
    cr: bool,
}

impl FrameLen {
    /// Counts `bytes` in, answering whether the frame still fits in `max`.
    fn count(&mut self, bytes: &[u8], max: usize) -> bool {
        for &byte in bytes {
            let cr = mem::replace(&mut self.cr, byte == b'\r');

            match byte {
                b'\n' if cr => continue,
                b'\r' | b'\n' if self.line == 0 => self.fields = 0,
                b'\r' | b'\n' => {
                    if !self.comment {
                        self.fields += self.line;
                    }
                    self.line = 0;
                }
                _ => {
                    if self.line == 0 {
                        self.comment = byte == b':';
                    }
                    self.line += 1;

                    if self.fields + self.line > max {
                        return false;
                    }
                }
            }
        }

        true
    }
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

    use crate::rfc8620::{event_source::subscribe::*, event_source::*, session::JmapSession};

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

    /// The shape Fastmail and Stalwart send, which appending to would
    /// subscribe to the literal `{types}`.
    #[test]
    fn subscribe_url_expands_the_uri_template() {
        let session = JmapSession {
            event_source_url: "https://jmap.example.org/eventsource/?types={types}&closeafter={closeafter}&ping={ping}".into(),
            ..synthetic_session()
        };
        let url = JmapEventSource::subscribe_url(
            &session,
            &["Email", "ContactCard"],
            30,
            JmapCloseAfter::State,
        );
        assert_eq!(
            url,
            "https://jmap.example.org/eventsource/?types=Email,ContactCard&closeafter=state&ping=30"
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
            "https://jmap.example.org/events?token=abc&types=*&closeafter=state&ping=15"
        );
    }

    #[test]
    fn request_host_keeps_the_port() {
        let session = JmapSession {
            event_source_url: "https://example.org:8443/sse".into(),
            ..synthetic_session()
        };
        let auth = SecretString::from("Bearer t".to_string());
        let shutdown = Arc::new(AtomicBool::new(false));
        let opts = JmapEventSourceOptions::default();
        let mut es =
            JmapEventSource::new(&session, &auth, &[], 30, JmapCloseAfter::No, shutdown, opts)
                .unwrap();

        let JmapCoroutineState::Yielded(JmapEventSourceYield::WantsWrite(bytes)) = es.resume(None)
        else {
            panic!("expected initial WantsWrite");
        };
        let req = core::str::from_utf8(&bytes).unwrap().to_ascii_lowercase();
        assert!(req.contains("\r\nhost: example.org:8443\r\n"), "{req}");
    }

    #[test]
    fn frame_of_another_event_is_ignored() {
        let mut es = streaming(JMAP_DEFAULT_MAX_FRAME_LEN);

        expect_wants_read(&mut es, &chunk(b"event: other\ndata: hello\n\n"));
    }

    #[test]
    fn frame_yields_its_event_id() {
        let mut es = streaming(JMAP_DEFAULT_MAX_FRAME_LEN);
        let frame =
            b"id: evt-1\nevent: state\ndata: {\"@type\":\"StateChange\",\"changed\":{}}\n\n";

        match es.resume(Some(&chunk(frame))) {
            JmapCoroutineState::Yielded(JmapEventSourceYield::Frame { id, .. }) => {
                assert_eq!(id.as_deref(), Some("evt-1"));
            }
            other => panic!("expected Frame yield, got {other:?}"),
        }
    }

    #[test]
    fn last_event_id_is_sent_and_checked() {
        let session = JmapSession {
            event_source_url: "https://example.org/sse".into(),
            ..synthetic_session()
        };
        let auth = SecretString::from("Bearer t".to_string());
        let subscribe = |id: &str| {
            let shutdown = Arc::new(AtomicBool::new(false));
            let opts = JmapEventSourceOptions {
                last_event_id: Some(id.into()),
            };
            JmapEventSource::new(&session, &auth, &[], 30, JmapCloseAfter::No, shutdown, opts)
        };

        let mut es = subscribe("evt-1").unwrap();
        let JmapCoroutineState::Yielded(JmapEventSourceYield::WantsWrite(bytes)) = es.resume(None)
        else {
            panic!("expected initial WantsWrite");
        };
        let req = core::str::from_utf8(&bytes).unwrap();
        assert!(req.contains("\r\nLast-Event-ID: evt-1\r\n"), "{req}");

        for id in ["evt\r\nX-Injected: 1", "evt\n", "evt\0"] {
            assert!(matches!(
                subscribe(id).err(),
                Some(JmapEventSourceError::InvalidLastEventId)
            ));
        }
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
            JmapEventSourceOptions::default(),
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
            JmapCoroutineState::Yielded(JmapEventSourceYield::Frame { change, .. }) => {
                assert_eq!(change.r#type, "StateChange");
                assert_eq!(change.changed["u1"]["Email"], "s1");
            }
            other => panic!("expected Frame yield, got {other:?}"),
        }
    }

    #[test]
    fn line_that_never_ends_returns_frame_too_large() {
        let mut es = streaming(JMAP_DEFAULT_MAX_FRAME_LEN);
        let body = [b'a'; 16 * 1024];

        for _ in 0..4 {
            expect_wants_read(&mut es, &chunk(&body));
        }

        match es.resume(Some(&chunk(b"a"))) {
            JmapCoroutineState::Complete(Err(JmapEventSourceError::FrameTooLarge(
                JMAP_DEFAULT_MAX_FRAME_LEN,
            ))) => {}
            other => panic!("expected FrameTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn data_lines_without_blank_line_return_frame_too_large() {
        let mut es = streaming(64);

        for _ in 0..4 {
            expect_wants_read(&mut es, &chunk(b"data: 0123456789\r\n"));
        }

        match es.resume(Some(&chunk(b"data: 0\n"))) {
            JmapCoroutineState::Complete(Err(JmapEventSourceError::FrameTooLarge(64))) => {}
            other => panic!("expected FrameTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn frames_and_comments_within_limit_keep_streaming() {
        let mut es = streaming(64);

        for _ in 0..4 {
            expect_wants_read(&mut es, &chunk(b": 0123456789 0123456789 0123456789\n"));
        }

        for _ in 0..4 {
            let frame = b": ping\ndata: {\"@type\":\"StateChange\",\"changed\":{}}\n\n";

            match es.resume(Some(&chunk(frame))) {
                JmapCoroutineState::Yielded(JmapEventSourceYield::Frame { .. }) => {}
                other => panic!("expected Frame yield, got {other:?}"),
            }
        }
    }

    #[test]
    fn chunk_of_small_frames_over_max_frame_len_streams_every_frame() {
        let mut es = streaming(JMAP_DEFAULT_MAX_FRAME_LEN);
        let frame = b"data: {\"@type\":\"StateChange\",\"changed\":{}}\n\n";
        let body = frame.repeat(2000);
        assert!(body.len() > JMAP_DEFAULT_MAX_FRAME_LEN);

        let chunk = chunk(&body);
        let mut arg = Some(chunk.as_slice());
        let mut frames = 0;

        loop {
            match es.resume(arg.take()) {
                JmapCoroutineState::Yielded(JmapEventSourceYield::Frame { .. }) => frames += 1,
                JmapCoroutineState::Yielded(JmapEventSourceYield::WantsRead) => break,
                other => panic!("expected Frame or WantsRead, got {other:?}"),
            }
        }

        assert_eq!(frames, 2000);
    }

    #[test]
    fn chunk_over_max_chunk_len_returns_chunk_too_large() {
        let mut es = streaming(JMAP_DEFAULT_MAX_FRAME_LEN);

        match es.resume(Some(b"100001\r\n")) {
            JmapCoroutineState::Complete(Err(JmapEventSourceError::ReadChunks(
                Http11ChunksReadStreamError::ChunkTooLarge(JMAP_DEFAULT_MAX_CHUNK_LEN),
            ))) => {}
            other => panic!("expected ChunkTooLarge, got {other:?}"),
        }
    }

    fn streaming(max_frame_len: usize) -> JmapEventSource {
        let session = JmapSession {
            event_source_url: "https://example.org/sse".into(),
            ..synthetic_session()
        };
        let auth = SecretString::from("Bearer t".to_string());
        let shutdown = Arc::new(AtomicBool::new(false));
        let opts = JmapEventSourceOptions::default();
        let mut es =
            JmapEventSource::new(&session, &auth, &[], 30, JmapCloseAfter::No, shutdown, opts)
                .unwrap();
        es.max_frame_len = max_frame_len;

        let JmapCoroutineState::Yielded(JmapEventSourceYield::WantsWrite(_)) = es.resume(None)
        else {
            panic!("expected initial WantsWrite");
        };

        let head = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        let JmapCoroutineState::Yielded(JmapEventSourceYield::WantsRead) = es.resume(Some(head))
        else {
            panic!("expected WantsRead after the head");
        };

        es
    }

    fn expect_wants_read(es: &mut JmapEventSource, bytes: &[u8]) {
        match es.resume(Some(bytes)) {
            JmapCoroutineState::Yielded(JmapEventSourceYield::WantsRead) => {}
            other => panic!("expected WantsRead, got {other:?}"),
        }
    }

    fn chunk(body: &[u8]) -> Vec<u8> {
        let mut chunk = format!("{:x}\r\n", body.len()).into_bytes();
        chunk.extend_from_slice(body);
        chunk.extend_from_slice(b"\r\n");
        chunk
    }
}
