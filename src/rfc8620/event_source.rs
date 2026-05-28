//! JMAP Event Source push channel (RFC 8620 §7.2 & §7.2.1).
//!
//! Servers advertise an SSE endpoint via [`JmapSession::event_source_url`].
//! A streaming GET against that URL yields a sequence of W3C SSE
//! frames; this module defines the JSON shape of the frame payloads
//! and provides [`parse_state_change`] to decode them.
//!
//! Transport (HTTP/1.1 streaming + SSE frame parsing) lives in
//! `io-http`'s `sse` module. JMAP push consumers compose
//! `io-http`'s `HttpClientStd::send_streaming` with the parser here.

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

/// Builds the JMAP push subscription URL from the session.
///
/// The returned URL points at the server's SSE endpoint with the
/// requested `types` filter (comma-separated JMAP type names),
/// `closeafter=no` (keep the connection open across notifications),
/// and `ping=<seconds>` to ask the server for keep-alive comment
/// frames at that cadence. `types` may be empty for "all types".
pub fn subscribe_url(session: &JmapSession, types: &[&str], ping_seconds: u64) -> String {
    let base = &session.event_source_url;
    let types = types.join(",");
    let sep = if base.contains('?') { '&' } else { '?' };
    format!("{base}{sep}types={types}&closeafter=no&ping={ping_seconds}")
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
        let url = subscribe_url(&session, &["Email", "EmailDelivery"], 30);
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
        let url = subscribe_url(&session, &[], 15);
        assert_eq!(
            url,
            "https://jmap.example.org/events?token=abc&types=&closeafter=no&ping=15"
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
}
