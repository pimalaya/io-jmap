//! JMAP Event Source push channel (RFC 8620 §7.2 & §7.2.1).
//!
//! Servers advertise an SSE endpoint via [`JmapSession::event_source_url`].
//! A streaming GET against that URL yields a sequence of W3C SSE
//! frames; this module defines:
//!
//! 1. The JSON shape of the frame payloads ([`StateChange`]) and
//!    [`parse_state_change`] to decode them;
//! 2. [`JmapEventSource`], an I/O-free streaming coroutine that
//!    composes io-http's HTTP/1.1 streaming primitives with the
//!    decoder above to yield one [`StateChange`] per push frame.
//!
//! [`JmapEventSource`] declares its own [`JmapEventSourceYield`]
//! (with an intermediate `Frame(StateChange)` variant) because the
//! streaming shape doesn't fit the I/O-only [`JmapYield`].

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::{
    mem,
    sync::atomic::{AtomicBool, Ordering},
};

use io_http::{
    coroutine::*,
    rfc9110::{headers::TRANSFER_ENCODING, request::HttpRequest},
    rfc9112::{
        chunk_stream::{
            Http11ReadChunksStream, Http11ReadChunksStreamError, Http11ReadChunksStreamYield,
        },
        read_headers::{Http11ReadHeaders, Http11ReadHeadersError, Http11ReadHeadersOutput},
    },
    sse::frame::{SseFrameParser, SseFrameParserYield},
};
use log::trace;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::coroutine::*;
use crate::rfc8620::session::JmapSession;

/// Type-state map for one JMAP account, keyed by JMAP type name
/// (`Email`, `Mailbox`, `EmailDelivery`, `Thread`, ...). The value is
/// the opaque state string; callers compare it against their stored
/// per-type checkpoint and call `<Type>/changes` when it differs.
pub type TypeStates = BTreeMap<String, String>;

/// JMAP `StateChange` push notification (RFC 8620 §7.2.1).
///
/// `changed` is keyed by account id; for each account, the inner
/// map gives the new opaque state for every JMAP type the server
/// considers changed since the last notification.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateChange {
    #[serde(rename = "@type", default = "default_type_tag")]
    pub r#type: String,
    #[serde(default)]
    pub changed: BTreeMap<String, TypeStates>,
}

const DEFAULT_TYPE_TAG: &str = "StateChange";

fn default_type_tag() -> String {
    DEFAULT_TYPE_TAG.to_string()
}

/// Errors from [`parse_state_change`].
#[derive(Debug, Error)]
pub enum EventSourceError {
    #[error("invalid JMAP StateChange JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("expected `@type: StateChange`, got `{0}`")]
    UnexpectedType(String),
}

/// Decodes the `data` field of one SSE frame as a JMAP `StateChange`
/// push notification. Empty or whitespace-only payloads return [`Ok`]
/// with an empty `changed` map; this lets callers treat keep-alive
/// comment frames uniformly with real state-change frames.
pub fn parse_state_change(data: &str) -> Result<StateChange, EventSourceError> {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return Ok(StateChange::default());
    }

    let change: StateChange = serde_json::from_str(trimmed)?;
    if change.r#type != DEFAULT_TYPE_TAG {
        return Err(EventSourceError::UnexpectedType(change.r#type));
    }

    Ok(change)
}

/// JMAP EventSource `closeafter` query value (RFC 8620 §7.2).
///
/// Controls when the server closes the streaming response:
/// - [`Self::No`] never; the connection stays open across
///   notifications. Lets the client receive many [`StateChange`]
///   frames over one TCP socket, but the socket is then unavailable
///   for parallel JMAP API calls (HTTP/1.1 is half-duplex while a
///   streaming response is in flight).
/// - [`Self::State`] after the first [`StateChange`] frame. Mimics
///   the IMAP IDLE pattern over HTTP/1.1: subscribe, receive one
///   state change, server closes the chunked response, the TCP
///   socket is then free for follow-up `Email/changes` +
///   `Email/get` POSTs on the same connection, then resubscribe.
///   Recommended for the unified [`JmapEventSource`] driver.
#[derive(Clone, Copy, Debug)]
pub enum CloseAfter {
    No,
    State,
}

impl CloseAfter {
    fn as_str(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::State => "state",
        }
    }
}

/// Builds the JMAP push subscription URL from the session.
///
/// The returned URL points at the server's SSE endpoint with the
/// requested `types` filter (comma-separated JMAP type names),
/// `closeafter=<close_after>` (see [`CloseAfter`]), and
/// `ping=<seconds>` to ask the server for keep-alive comment frames
/// at that cadence. `types` may be empty for "all types".
pub fn subscribe_url(
    session: &JmapSession,
    types: &[&str],
    ping_seconds: u64,
    close_after: CloseAfter,
) -> String {
    let base = &session.event_source_url;
    let types = types.join(",");
    let sep = if base.contains('?') { '&' } else { '?' };
    let close_after = close_after.as_str();
    format!("{base}{sep}types={types}&closeafter={close_after}&ping={ping_seconds}")
}

/// Errors raised by [`JmapEventSource::resume`].
///
/// Layered: HTTP transport errors (head parse / chunked body),
/// SSE-payload decode errors ([`EventSourceError`] via
/// [`parse_state_change`]), plus a small set of stream-shape errors
/// (`HttpStatus`, `NotChunked`, `InvalidUrl`).
#[derive(Debug, Error)]
pub enum JmapEventSourceError {
    #[error(transparent)]
    ReadHeaders(#[from] Http11ReadHeadersError),
    #[error(transparent)]
    ReadChunks(#[from] Http11ReadChunksStreamError),
    #[error(transparent)]
    DecodeFrame(#[from] EventSourceError),
    #[error("event source returned HTTP status `{0}`")]
    HttpStatus(u16),
    #[error("event source response must be `Transfer-Encoding: chunked`")]
    NotChunked,
    #[error("event source URL `{0}` is not parseable")]
    InvalidUrl(String),
}

/// Per-step yield emitted by [`JmapEventSource::resume`].
///
/// Extends the standard [`JmapYield`] with [`Self::Frame`], the
/// streaming intermediate value. Terminal success is
/// [`JmapCoroutineState::Complete`] with `Ok(())`.
#[derive(Debug)]
pub enum JmapEventSourceYield {
    /// One decoded push notification, ready to be diffed against the
    /// caller's per-type state cache. Empty-data SSE frames (pings,
    /// comment-only frames) surface as the default [`StateChange`]
    /// with an empty `changed` map; callers can use that as a
    /// keep-alive signal.
    Frame(StateChange),
    /// The driver should read more bytes from the socket and feed
    /// them back via `arg` on the next resume.
    WantsRead,
    /// The driver should write these bytes to the socket; the next
    /// resume typically takes `arg = None`.
    WantsWrite(Vec<u8>),
}

/// I/O-free streaming coroutine for the JMAP `EventSource` push
/// channel.
///
/// Composes [`Http11ReadHeaders`] (response head) + [`Http11ReadChunksStream`]
/// (chunked body framing) + [`SseFrameParser`] (W3C SSE line protocol)
/// + [`parse_state_change`] (RFC 8620 §7.2.1 payload decode) into a
/// single state machine that yields one [`StateChange`] per push
/// notification.
///
/// Cooperative shutdown: callers share a [`AtomicBool`] flag with
/// [`Self::new`]. The coroutine polls it at the top of every
/// [`Self::resume`] and transitions to terminal `Complete(Ok(()))`
/// when set. The caller's I/O driver has to honour the flag too if
/// it wants to interrupt an in-progress blocking socket read.
pub struct JmapEventSource {
    state: EventSourceState,
    shutdown: Arc<AtomicBool>,
}

impl JmapEventSource {
    /// Builds the subscription URL from the session, prepares the
    /// initial `GET` request bytes (with `Authorization` and `Accept:
    /// text/event-stream`), and returns a coroutine ready to be
    /// driven.
    ///
    /// `types` filters by JMAP data type (`["Email", "Mailbox"]`,
    /// …); empty for "all types". `ping_seconds` asks the server to
    /// emit synthetic comment frames at that cadence so the channel
    /// has a heartbeat. `close_after` controls the subscription
    /// lifecycle:
    ///
    /// - [`CloseAfter::No`]: long-lived stream, many frames per
    ///   coroutine instance. Terminal `Complete(Ok(()))` only fires
    ///   on shutdown or transport EOF.
    /// - [`CloseAfter::State`]: one-shot cycle, server closes after
    ///   the first frame. Terminal `Complete(Ok(()))` fires after
    ///   that close, and the caller is expected to construct a fresh
    ///   [`JmapEventSource`] for the next cycle once any follow-up
    ///   `Email/changes` + `Email/get` POSTs on the freed socket
    ///   have completed. This is the IMAP-IDLE analog (subscribe →
    ///   wait → unsubscribe → query → resubscribe) and is the right
    ///   choice when the caller needs to multiplex the connection.
    ///
    /// `shutdown` is shared with the caller; flip it to ask the
    /// coroutine to wind down at the next resume.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        types: &[&str],
        ping_seconds: u64,
        close_after: CloseAfter,
        shutdown: Arc<AtomicBool>,
    ) -> Result<Self, JmapEventSourceError> {
        let url_str = subscribe_url(session, types, ping_seconds, close_after);
        let url = Url::parse(&url_str).map_err(|_| JmapEventSourceError::InvalidUrl(url_str))?;

        let host = url.host_str().unwrap_or("localhost");
        let request = HttpRequest::get(url.clone())
            .header("Host", host)
            .header("Accept", "text/event-stream")
            .header("Authorization", http_auth.expose_secret());

        trace!("prepare JMAP event source subscription to {url}");

        Ok(Self {
            state: EventSourceState::SendingRequest(request.to_http_11_vec()),
            shutdown,
        })
    }
}

impl JmapCoroutine for JmapEventSource {
    type Yield = JmapEventSourceYield;
    type Return = Result<(), JmapEventSourceError>;

    /// Advances the coroutine.
    ///
    /// Pass [`None`] on the initial call. Pass `Some(data)` with
    /// bytes read from the socket after a
    /// [`JmapEventSourceYield::WantsRead`]. Pass `Some(&[])` to
    /// signal EOF; on the head stage this surfaces as an error, on
    /// the streaming stage it surfaces as terminal `Complete(Ok(()))`.
    fn resume(&mut self, mut arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        if self.shutdown.load(Ordering::SeqCst) {
            self.state = EventSourceState::Done;
            return JmapCoroutineState::Complete(Ok(()));
        }

        loop {
            match &mut self.state {
                EventSourceState::SendingRequest(_) => {
                    let EventSourceState::SendingRequest(bytes) = mem::replace(
                        &mut self.state,
                        EventSourceState::ReadingHead(Http11ReadHeaders::default()),
                    ) else {
                        unreachable!()
                    };
                    return JmapCoroutineState::Yielded(JmapEventSourceYield::WantsWrite(bytes));
                }
                EventSourceState::ReadingHead(head) => match head.resume(arg.take()) {
                    HttpCoroutineState::Yielded(HttpYield::WantsRead) => {
                        return JmapCoroutineState::Yielded(JmapEventSourceYield::WantsRead);
                    }
                    HttpCoroutineState::Yielded(HttpYield::WantsWrite(_)) => {
                        unreachable!("Http11ReadHeaders never writes");
                    }
                    HttpCoroutineState::Complete(Err(err)) => {
                        return JmapCoroutineState::Complete(Err(err.into()));
                    }
                    HttpCoroutineState::Complete(Ok(Http11ReadHeadersOutput {
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
                            .header(TRANSFER_ENCODING)
                            .is_some_and(|enc| enc.eq_ignore_ascii_case("chunked"));
                        if !chunked {
                            return JmapCoroutineState::Complete(Err(
                                JmapEventSourceError::NotChunked,
                            ));
                        }
                        // The HTTP head reader may have over-read past
                        // the blank line and hold the start of the
                        // chunked body in `remaining`. Those bytes are
                        // chunk-encoded; prime the chunk decoder with
                        // them so the SSE parser only ever sees
                        // decoded chunk bodies, never `<hex>\r\n` size
                        // headers.
                        let mut chunks = Http11ReadChunksStream::default();
                        let pending = if remaining.is_empty() {
                            None
                        } else {
                            match chunks.resume(Some(&remaining)) {
                                HttpCoroutineState::Yielded(
                                    Http11ReadChunksStreamYield::Frame { body },
                                ) => Some(body),
                                HttpCoroutineState::Yielded(
                                    Http11ReadChunksStreamYield::WantsRead,
                                ) => None,
                                HttpCoroutineState::Complete(Ok(_)) => {
                                    self.state = EventSourceState::Done;
                                    return JmapCoroutineState::Complete(Ok(()));
                                }
                                HttpCoroutineState::Complete(Err(err)) => {
                                    return JmapCoroutineState::Complete(Err(err.into()));
                                }
                            }
                        };
                        self.state = EventSourceState::Streaming {
                            chunks,
                            parser: SseFrameParser::default(),
                            pending,
                        };
                        // Loop into the streaming arm; subsequent
                        // chunk.resume calls keep parsing from the
                        // primed buffer before asking for more bytes.
                    }
                },
                EventSourceState::Streaming {
                    chunks,
                    parser,
                    pending,
                } => {
                    let parser_input = pending.take();
                    match parser.resume(parser_input.as_deref()) {
                        HttpCoroutineState::Yielded(SseFrameParserYield::Frame(frame)) => {
                            return match parse_state_change(&frame.data) {
                                Ok(change) => {
                                    JmapCoroutineState::Yielded(JmapEventSourceYield::Frame(change))
                                }
                                Err(err) => JmapCoroutineState::Complete(Err(err.into())),
                            };
                        }
                        HttpCoroutineState::Yielded(SseFrameParserYield::WantsBytes) => {
                            match chunks.resume(arg.take()) {
                                HttpCoroutineState::Yielded(
                                    Http11ReadChunksStreamYield::Frame { body },
                                ) => {
                                    *pending = Some(body);
                                    // Loop to feed the parser.
                                }
                                HttpCoroutineState::Complete(Ok(_)) => {
                                    self.state = EventSourceState::Done;
                                    return JmapCoroutineState::Complete(Ok(()));
                                }
                                HttpCoroutineState::Yielded(
                                    Http11ReadChunksStreamYield::WantsRead,
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
                EventSourceState::Done => return JmapCoroutineState::Complete(Ok(())),
            }
        }
    }
}

/// Internal progression state of [`JmapEventSource`].
enum EventSourceState {
    /// Initial: yield the GET request bytes once, transition to
    /// reading the response head.
    SendingRequest(Vec<u8>),
    /// Driving [`Http11ReadHeaders`] on the response.
    ReadingHead(Http11ReadHeaders),
    /// Streaming: pump [`Http11ReadChunksStream`] for body chunks,
    /// feed them into [`SseFrameParser`] one at a time, decode each
    /// dispatched frame into a [`StateChange`].
    Streaming {
        chunks: Http11ReadChunksStream,
        parser: SseFrameParser,
        /// Decoded chunk body waiting to be fed to the SSE parser.
        pending: Option<Vec<u8>>,
    },
    /// Terminal (shutdown or chunked-transfer stream-over).
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_state_change() {
        let json = r#"{"@type":"StateChange","changed":{"u1":{"Email":"s1"}}}"#;
        let change = parse_state_change(json).unwrap();
        assert_eq!(change.r#type, DEFAULT_TYPE_TAG);
        assert_eq!(change.changed.len(), 1);
        assert_eq!(change.changed["u1"]["Email"], "s1");
    }

    #[test]
    fn parses_multi_account_multi_type() {
        let json = r#"{
            "@type": "StateChange",
            "changed": {
                "acc-a": {"Email": "e1", "Mailbox": "m1"},
                "acc-b": {"Email": "e2"}
            }
        }"#;
        let change = parse_state_change(json).unwrap();
        assert_eq!(change.changed.len(), 2);
        assert_eq!(change.changed["acc-a"]["Mailbox"], "m1");
        assert_eq!(change.changed["acc-b"]["Email"], "e2");
    }

    #[test]
    fn empty_data_is_keep_alive() {
        let change = parse_state_change("").unwrap();
        assert!(change.changed.is_empty());

        let change = parse_state_change("   \n  ").unwrap();
        assert!(change.changed.is_empty());
    }

    #[test]
    fn wrong_type_field_rejected() {
        let json = r#"{"@type":"NotAStateChange","changed":{}}"#;
        match parse_state_change(json) {
            Err(EventSourceError::UnexpectedType(t)) => assert_eq!(t, "NotAStateChange"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn invalid_json_rejected() {
        match parse_state_change("{not json") {
            Err(EventSourceError::InvalidJson(_)) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn missing_changed_field_defaults_to_empty() {
        let json = r#"{"@type":"StateChange"}"#;
        let change = parse_state_change(json).unwrap();
        assert!(change.changed.is_empty());
    }

    #[test]
    fn subscribe_url_appends_query_params() {
        let session = JmapSession {
            event_source_url: "https://jmap.example.org/events".into(),
            ..synthetic_session()
        };
        let url = subscribe_url(&session, &["Email", "EmailDelivery"], 30, CloseAfter::No);
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
        let url = subscribe_url(&session, &[], 15, CloseAfter::State);
        assert_eq!(
            url,
            "https://jmap.example.org/events?token=abc&types=&closeafter=state&ping=15"
        );
    }

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

    // Regression: when the HTTP head reader over-reads past
    // `\r\n\r\n` and the leftover bytes contain the start of the
    // chunked body, those bytes must be routed through the chunk
    // decoder. The original implementation fed them straight into
    // the SSE parser, which saw `<hex chunk size>\r\n` lines as
    // unknown fields and (crucially) split a multi-chunk SSE field
    // at the `<hex>\r\n` boundary, truncating the field value and
    // breaking JSON parsing downstream.
    //
    // The body here splits `data:` across two chunks so the broken
    // path produces invalid JSON and the test fails with the
    // original code; the fix routes leftover bytes through the
    // chunk decoder so the SSE parser sees the two halves joined.
    #[test]
    fn streaming_head_leftover_is_chunk_decoded() {
        use alloc::string::ToString;

        let session = JmapSession {
            event_source_url: "https://example.org/sse".into(),
            ..synthetic_session()
        };
        let auth = SecretString::from("Bearer t".to_string());
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut es =
            JmapEventSource::new(&session, &auth, &["Email"], 30, CloseAfter::State, shutdown)
                .unwrap();

        // Drain the initial GET-request write so the next resume
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
        let mut wire = head.as_bytes().to_vec();
        wire.extend_from_slice(chunked.as_bytes());

        // Head + the full multi-chunk body arrive in the same socket
        // read: matches the Fastmail trace shape and the read
        // boundary that triggered the bug.
        match es.resume(Some(&wire)) {
            JmapCoroutineState::Yielded(JmapEventSourceYield::Frame(change)) => {
                assert_eq!(change.r#type, DEFAULT_TYPE_TAG);
                assert_eq!(change.changed["u1"]["Email"], "s1");
            }
            other => panic!("expected Frame yield, got {other:?}"),
        }
    }
}
