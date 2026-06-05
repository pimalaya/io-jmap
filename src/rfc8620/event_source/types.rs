//! JMAP Event Source data types (RFC 8620 §7.2.1): push payload, per-account
//! type-state map, `closeafter` value, and the parser-level error.

use alloc::{collections::BTreeMap, string::String};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::utils::default_type_tag;

/// Type-state map for one JMAP account, keyed by JMAP type name (`Email`,
/// `Mailbox`, …); the value is the opaque state string. Callers diff it
/// against their stored checkpoint and call `<Type>/changes` on a mismatch.
pub type TypeStates = BTreeMap<String, String>;

/// JMAP `StateChange` push notification (RFC 8620 §7.2.1).
///
/// `changed` is keyed by account id, then JMAP type, then opaque new state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateChange {
    #[serde(rename = "@type", default = "default_type_tag")]
    pub r#type: String,
    #[serde(default)]
    pub changed: BTreeMap<String, TypeStates>,
}

/// Failure causes from
/// [`parse_state_change`](super::utils::parse_state_change).
#[derive(Debug, Error)]
pub enum EventSourceError {
    #[error("Invalid JMAP StateChange JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("Expected @type StateChange, got {0}")]
    UnexpectedType(String),
}

/// JMAP EventSource `closeafter` query value (RFC 8620 §7.2): when the server
/// closes the streaming response.
#[derive(Clone, Copy, Debug)]
pub enum CloseAfter {
    /// Never close: stream many [`StateChange`] frames over one socket. The
    /// socket is unavailable for parallel JMAP POSTs while the stream is open.
    No,
    /// Close after the first [`StateChange`]: frees the socket for follow-up
    /// `*/changes` + `*/get` POSTs, then resubscribe (IMAP IDLE-like pattern).
    /// Recommended for [`JmapEventSource`](super::subscribe::JmapEventSource).
    State,
}

impl CloseAfter {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::State => "state",
        }
    }
}
