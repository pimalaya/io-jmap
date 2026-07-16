//! JMAP Event Source push channel (RFC 8620 §7.3 & §7.1).
//!
//! A streaming GET against
//! [`JmapSession::event_source_url`](crate::rfc8620::session::JmapSession::event_source_url)
//! yields W3C SSE frames carrying [`JmapStateChange`] payloads.

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod subscribe;

/// Wire value of the `@type` property of a StateChange object.
const DEFAULT_TYPE_TAG: &str = "StateChange";

fn default_type_tag() -> String {
    DEFAULT_TYPE_TAG.to_string()
}

/// Type-state map for one JMAP account, keyed by JMAP type name (`Email`,
/// `Mailbox`, …); the value is the opaque state string. Callers diff it
/// against their stored checkpoint and call `<Type>/changes` on a mismatch.
pub type JmapTypeStates = BTreeMap<String, String>;

/// JMAP StateChange push notification (RFC 8620 §7.1).
///
/// `changed` is keyed by account id, then JMAP type, then opaque new state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JmapStateChange {
    /// The `@type` tag of the push object, always `StateChange`.
    #[serde(rename = "@type", default = "default_type_tag")]
    pub r#type: String,
    /// The new type states, keyed by account id.
    #[serde(default)]
    pub changed: BTreeMap<String, JmapTypeStates>,
}

impl JmapStateChange {
    /// Decodes one SSE frame's `data` field as a JMAP StateChange. Empty or
    /// whitespace-only payloads return an empty `changed` map (keep-alive).
    pub fn parse(data: &str) -> Result<Self, JmapStateChangeParseError> {
        let trimmed = data.trim();
        if trimmed.is_empty() {
            return Ok(Self::default());
        }

        let change: Self = serde_json::from_str(trimmed)?;
        if change.r#type != DEFAULT_TYPE_TAG {
            return Err(JmapStateChangeParseError::UnexpectedType(change.r#type));
        }

        Ok(change)
    }
}

/// Failure causes from [`JmapStateChange::parse`].
#[derive(Debug, Error)]
pub enum JmapStateChangeParseError {
    /// The payload is not valid JSON.
    #[error("Invalid StateChange JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    /// The payload's `@type` tag is not `StateChange`.
    #[error("Expected @type StateChange, got {0}")]
    UnexpectedType(String),
}

/// JMAP EventSource `closeafter` query value (RFC 8620 §7.3): when the server
/// closes the streaming response.
#[derive(Clone, Copy, Debug)]
pub enum JmapCloseAfter {
    /// Never close: stream many [`JmapStateChange`] frames over one socket.
    /// The socket is unavailable for parallel JMAP POSTs while the stream is
    /// open.
    No,
    /// Close after the first [`JmapStateChange`]: frees the socket for
    /// follow-up `*/changes` + `*/get` POSTs, then resubscribe (IMAP
    /// IDLE-like pattern). Recommended for
    /// [`JmapEventSource`](crate::rfc8620::event_source::subscribe::JmapEventSource).
    State,
}

impl JmapCloseAfter {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::State => "state",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rfc8620::event_source::*;

    #[test]
    fn parses_minimal_state_change() {
        let json = r#"{"@type":"StateChange","changed":{"u1":{"Email":"s1"}}}"#;
        let change = JmapStateChange::parse(json).unwrap();
        assert_eq!(change.r#type, "StateChange");
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
        let change = JmapStateChange::parse(json).unwrap();
        assert_eq!(change.changed.len(), 2);
        assert_eq!(change.changed["acc-a"]["Mailbox"], "m1");
        assert_eq!(change.changed["acc-b"]["Email"], "e2");
    }

    #[test]
    fn empty_data_is_keep_alive() {
        let change = JmapStateChange::parse("").unwrap();
        assert!(change.changed.is_empty());

        let change = JmapStateChange::parse("   \n  ").unwrap();
        assert!(change.changed.is_empty());
    }

    #[test]
    fn wrong_type_field_rejected() {
        let json = r#"{"@type":"NotAStateChange","changed":{}}"#;
        match JmapStateChange::parse(json) {
            Err(JmapStateChangeParseError::UnexpectedType(t)) => assert_eq!(t, "NotAStateChange"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn invalid_json_rejected() {
        match JmapStateChange::parse("{not json") {
            Err(JmapStateChangeParseError::InvalidJson(_)) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn missing_changed_field_defaults_to_empty() {
        let json = r#"{"@type":"StateChange"}"#;
        let change = JmapStateChange::parse(json).unwrap();
        assert!(change.changed.is_empty());
    }
}
