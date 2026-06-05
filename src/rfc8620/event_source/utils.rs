//! Helpers for the JMAP Event Source channel: decode an SSE frame as a
//! [`JmapStateChange`], and build the subscription URL from a [`JmapSession`].

use alloc::{
    format,
    string::{String, ToString},
};

use crate::rfc8620::JmapSession;

use super::types::{JmapCloseAfter, JmapStateChange, JmapStateChangeParseError};

pub(super) const DEFAULT_TYPE_TAG: &str = "StateChange";

pub(super) fn default_type_tag() -> String {
    DEFAULT_TYPE_TAG.to_string()
}

/// Decodes one SSE frame's `data` field as a JMAP `StateChange`. Empty or
/// whitespace-only payloads return an empty `changed` map (keep-alive).
pub fn parse_state_change(data: &str) -> Result<JmapStateChange, JmapStateChangeParseError> {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return Ok(JmapStateChange::default());
    }

    let change: JmapStateChange = serde_json::from_str(trimmed)?;
    if change.r#type != DEFAULT_TYPE_TAG {
        return Err(JmapStateChangeParseError::UnexpectedType(change.r#type));
    }

    Ok(change)
}

/// Builds the JMAP push subscription URL: `event_source_url` plus
/// `types=<csv>`, `closeafter=<v>` (see [`JmapCloseAfter`]) and
/// `ping=<seconds>`.  `types` may be empty for "all types".
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

#[cfg(test)]
mod tests {
    use alloc::{collections::BTreeMap, string::String};

    use super::*;
    use crate::rfc8620::JmapSession;

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
            Err(JmapStateChangeParseError::UnexpectedType(t)) => assert_eq!(t, "NotAStateChange"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn invalid_json_rejected() {
        match parse_state_change("{not json") {
            Err(JmapStateChangeParseError::InvalidJson(_)) => {}
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
        let url = subscribe_url(
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
        let url = subscribe_url(&session, &[], 15, JmapCloseAfter::State);
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
}
