//! Tests for JMAP for Calendars (draft-ietf-jmap-calendars).
//!
//! All tests resume JMAP coroutines against pre-crafted in-memory HTTP
//! response buffers. No network connection is made. Each helper returns
//! the bytes the coroutine asked to write next to its terminal state,
//! so a test can assert on the request it built as well as on the
//! response it decoded.

use io_jmap::{
    calendars::{
        calendar::{
            JmapCalendarAvailability,
            changes::{JmapCalendarChanges, JmapCalendarChangesError, JmapCalendarChangesOptions},
            get::{
                JmapCalendarGet, JmapCalendarGetError, JmapCalendarGetOptions,
                JmapCalendarGetOutput, JmapCalendarProperty,
            },
        },
        calendar_event::{
            JmapCalendarEvent,
            changes::{
                JmapCalendarEventChanges, JmapCalendarEventChangesError,
                JmapCalendarEventChangesOptions,
            },
            get::{
                JmapCalendarEventGet, JmapCalendarEventGetError, JmapCalendarEventGetOptions,
                JmapCalendarEventGetOutput,
            },
            query::{
                JmapCalendarEventFilter, JmapCalendarEventQuery, JmapCalendarEventQueryError,
                JmapCalendarEventQueryOptions, JmapCalendarEventQueryOutput,
                JmapCalendarEventSortComparator, JmapCalendarEventSortProperty,
            },
        },
    },
    coroutine::*,
    rfc8620::{changes::JmapChangesOutput, session::JmapSession},
};
use secrecy::SecretString;

/// A minimal session object with a single calendar account `acc1`.
const SESSION_JSON: &[u8] = br#"{
  "username": "user@example.com",
  "accounts": {
    "acc1": {
      "name": "Test Account",
      "isPersonal": true,
      "isReadOnly": false,
      "accountCapabilities": {}
    }
  },
  "primaryAccounts": {
    "urn:ietf:params:jmap:core": "acc1",
    "urn:ietf:params:jmap:calendars": "acc1"
  },
  "capabilities": {
    "urn:ietf:params:jmap:core": {},
    "urn:ietf:params:jmap:calendars": {}
  },
  "apiUrl": "http://example.com/jmap/api/",
  "downloadUrl": "http://example.com/jmap/download/{accountId}/{blobId}/{name}?accept={type}",
  "uploadUrl": "http://example.com/jmap/upload/{accountId}/",
  "eventSourceUrl": "http://example.com/jmap/eventsource/",
  "state": "s1"
}"#;

fn make_session() -> JmapSession {
    serde_json::from_slice(SESSION_JSON).expect("parse test session")
}

fn make_token() -> SecretString {
    SecretString::from("Bearer test-token")
}

fn http_ok(body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

type Outcome<T, E> = (String, JmapCoroutineState<JmapYield, Result<T, E>>);

fn run_calendar_get(
    http_response_bytes: &[u8],
    opts: JmapCalendarGetOptions,
) -> Outcome<JmapCalendarGetOutput, JmapCalendarGetError> {
    let session = make_session();
    let token = make_token();
    let mut coroutine = JmapCalendarGet::new(&session, &token, opts).unwrap();
    let mut request = String::new();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
                request.push_str(&String::from_utf8_lossy(&bytes));
            }
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return (request, any),
        }
    }
}

fn run_calendar_changes(
    http_response_bytes: &[u8],
) -> Outcome<JmapChangesOutput, JmapCalendarChangesError> {
    let session = make_session();
    let token = make_token();
    let mut coroutine = JmapCalendarChanges::new(
        &session,
        &token,
        "s1",
        JmapCalendarChangesOptions { max_changes: None },
    )
    .unwrap();
    let mut request = String::new();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
                request.push_str(&String::from_utf8_lossy(&bytes));
            }
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return (request, any),
        }
    }
}

fn run_calendar_event_get(
    http_response_bytes: &[u8],
    opts: JmapCalendarEventGetOptions,
) -> Outcome<JmapCalendarEventGetOutput, JmapCalendarEventGetError> {
    let session = make_session();
    let token = make_token();
    let mut coroutine = JmapCalendarEventGet::new(&session, &token, opts).unwrap();
    let mut request = String::new();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
                request.push_str(&String::from_utf8_lossy(&bytes));
            }
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return (request, any),
        }
    }
}

fn run_calendar_event_query(
    http_response_bytes: &[u8],
    opts: JmapCalendarEventQueryOptions,
) -> Outcome<JmapCalendarEventQueryOutput, JmapCalendarEventQueryError> {
    let session = make_session();
    let token = make_token();
    let mut coroutine = JmapCalendarEventQuery::new(&session, &token, opts).unwrap();
    let mut request = String::new();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
                request.push_str(&String::from_utf8_lossy(&bytes));
            }
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return (request, any),
        }
    }
}

fn run_calendar_event_changes(
    http_response_bytes: &[u8],
) -> Outcome<JmapChangesOutput, JmapCalendarEventChangesError> {
    let session = make_session();
    let token = make_token();
    let mut coroutine = JmapCalendarEventChanges::new(
        &session,
        &token,
        "s1",
        JmapCalendarEventChangesOptions {
            max_changes: Some(42),
        },
    )
    .unwrap();
    let mut request = String::new();
    let mut arg: Option<&[u8]> = None;

    loop {
        match coroutine.resume(arg.take()) {
            JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
                request.push_str(&String::from_utf8_lossy(&bytes));
            }
            JmapCoroutineState::Yielded(JmapYield::WantsRead) => arg = Some(http_response_bytes),
            any => return (request, any),
        }
    }
}

#[test]
fn calendar_get_ok() {
    let body = br##"{
      "methodResponses": [
        ["Calendar/get", {
          "state": "s2",
          "list": [
            {
              "id": "cal1",
              "name": "Personal",
              "description": "day to day",
              "color": "#aabbcc",
              "sortOrder": 3,
              "isDefault": true,
              "isSubscribed": true,
              "isVisible": false,
              "includeInAvailability": "attending",
              "timeZone": "Europe/Paris",
              "defaultAlertsWithTime": {
                "a1": {"@type": "Alert", "trigger": {"@type": "OffsetTrigger"}}
              },
              "myRights": {
                "mayReadFreeBusy": true,
                "mayReadItems": true,
                "mayWriteOwn": true,
                "mayRSVP": true
              }
            },
            {"id": "cal2", "name": "Work"}
          ],
          "notFound": ["cal3"]
        }, "c0"]
      ],
      "sessionState": "s2"
    }"##;

    let opts = JmapCalendarGetOptions {
        ids: Some(vec!["cal1".into(), "cal2".into(), "cal3".into()]),
        properties: Some(vec![
            JmapCalendarProperty::Name,
            JmapCalendarProperty::IncludeInAvailability,
            JmapCalendarProperty::MyRights,
        ]),
    };

    match run_calendar_get(&http_ok(body), opts) {
        (request, JmapCoroutineState::Complete(Ok(out))) => {
            assert!(
                request.contains(r#""properties":["name","includeInAvailability","myRights"]"#),
                "properties should serialize as their camelCase wire names: {request}"
            );
            assert!(
                request.contains(
                    r#""using":["urn:ietf:params:jmap:core","urn:ietf:params:jmap:calendars""#
                ),
                "the calendars capability should be requested: {request}"
            );

            assert_eq!(out.calendars.len(), 2);
            assert_eq!(out.not_found, vec!["cal3".to_string()]);
            assert_eq!(out.new_state, "s2");

            let cal = &out.calendars[0];
            assert_eq!(cal.name.as_deref(), Some("Personal"));
            assert_eq!(cal.color.as_deref(), Some("#aabbcc"));
            assert_eq!(cal.sort_order, 3);
            assert!(cal.is_default);
            assert_eq!(cal.is_visible, Some(false));
            assert_eq!(
                cal.include_in_availability,
                JmapCalendarAvailability::Attending
            );
            assert_eq!(cal.time_zone.as_deref(), Some("Europe/Paris"));
            assert!(
                cal.default_alerts_with_time
                    .as_ref()
                    .unwrap()
                    .contains_key("a1")
            );
            assert!(cal.my_rights.may_rsvp);
            assert!(!cal.my_rights.may_write_all);

            let work = &out.calendars[1];
            assert_eq!(work.name.as_deref(), Some("Work"));
            assert_eq!(
                work.include_in_availability,
                JmapCalendarAvailability::All,
                "an absent includeInAvailability falls back to the draft default"
            );
            assert_eq!(work.is_visible, None, "an absent isVisible stays unknown");
        }
        (_, other) => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn calendar_get_method_error() {
    let body = br#"{
      "methodResponses": [
        ["error", {"type": "invalidArguments", "description": "unknown property"}, "c0"]
      ],
      "sessionState": "s1"
    }"#;

    match run_calendar_get(&http_ok(body), JmapCalendarGetOptions::default()) {
        (_, JmapCoroutineState::Complete(Err(JmapCalendarGetError::Get(err)))) => {
            assert!(err.to_string().contains("invalidArguments"), "{err}");
        }
        (_, other) => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn calendar_changes_ok() {
    let body = br#"{
      "methodResponses": [
        ["Calendar/changes", {
          "oldState": "s1",
          "newState": "s2",
          "hasMoreChanges": false,
          "created": ["cal9"],
          "updated": [],
          "destroyed": ["cal8"]
        }, "c0"]
      ],
      "sessionState": "s2"
    }"#;

    match run_calendar_changes(&http_ok(body)) {
        (request, JmapCoroutineState::Complete(Ok(out))) => {
            assert!(request.contains(r#""sinceState":"s1""#), "{request}");
            assert_eq!(out.new_state, "s2");
            assert!(!out.has_more_changes);
            assert_eq!(out.created, vec!["cal9".to_string()]);
            assert_eq!(out.destroyed, vec!["cal8".to_string()]);
        }
        (_, other) => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn calendar_event_get_keeps_jscalendar_payload_raw() {
    let body = br#"{
      "methodResponses": [
        ["CalendarEvent/get", {
          "state": "s2",
          "list": [
            {
              "id": "ev1",
              "baseEventId": "ev0",
              "calendarIds": {"cal1": true},
              "isDraft": false,
              "isOrigin": true,
              "utcStart": "2026-08-10T08:00:00Z",
              "utcEnd": "2026-08-10T09:00:00Z",
              "@type": "Event",
              "uid": "abc-123",
              "title": "Standup",
              "start": "2026-08-10T10:00:00",
              "timeZone": "Europe/Paris",
              "duration": "PT1H",
              "recurrenceRules": [{"@type": "RecurrenceRule", "frequency": "daily"}]
            }
          ],
          "notFound": []
        }, "c0"]
      ],
      "sessionState": "s2"
    }"#;

    let opts = JmapCalendarEventGetOptions {
        ids: Some(vec!["ev1".into()]),
        recurrence_overrides_before: Some("2026-09-01T00:00:00Z".into()),
        reduce_participants: true,
        time_zone: Some("Europe/Paris".into()),
        ..Default::default()
    };

    match run_calendar_event_get(&http_ok(body), opts) {
        (request, JmapCoroutineState::Complete(Ok(out))) => {
            assert!(
                request.contains(r#""recurrenceOverridesBefore":"2026-09-01T00:00:00Z""#),
                "the expansion window is an argument the generic get has no notion of: {request}"
            );
            assert!(
                request.contains(r#""reduceParticipants":true"#),
                "{request}"
            );
            assert!(
                request.contains(r#""timeZone":"Europe/Paris""#),
                "{request}"
            );
            assert!(
                !request.contains("recurrenceOverridesAfter"),
                "unset arguments stay off the wire: {request}"
            );

            assert_eq!(out.events.len(), 1);
            let event = &out.events[0];
            assert_eq!(event.id.as_deref(), Some("ev1"));
            assert_eq!(event.base_event_id.as_deref(), Some("ev0"));
            assert_eq!(event.calendar_ids.get("cal1"), Some(&true));
            assert!(!event.is_draft);
            assert!(event.is_origin);
            assert_eq!(event.utc_start.as_deref(), Some("2026-08-10T08:00:00Z"));
            assert_eq!(event.utc_end.as_deref(), Some("2026-08-10T09:00:00Z"));

            assert_eq!(event.event["uid"], "abc-123");
            assert_eq!(event.event["title"], "Standup");
            assert_eq!(event.event["duration"], "PT1H");
            assert!(
                event.event["recurrenceRules"].is_array(),
                "the JSCalendar payload is passed through untouched"
            );
            assert!(
                !event.event.contains_key("utcStart"),
                "the JMAP properties are pulled out of the raw payload"
            );
        }
        (_, other) => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn calendar_event_get_round_trips() {
    let body = br#"{
      "methodResponses": [
        ["CalendarEvent/get", {
          "state": "s2",
          "list": [
            {
              "id": "ev1",
              "calendarIds": {"cal1": true},
              "@type": "Event",
              "uid": "abc-123",
              "title": "Standup"
            }
          ],
          "notFound": []
        }, "c0"]
      ],
      "sessionState": "s2"
    }"#;

    let (_, state) = run_calendar_event_get(&http_ok(body), JmapCalendarEventGetOptions::default());
    let JmapCoroutineState::Complete(Ok(out)) = state else {
        panic!("unexpected result: {state:?}");
    };

    let json = serde_json::to_string(&out.events[0]).unwrap();
    let back: JmapCalendarEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(back.id.as_deref(), Some("ev1"));
    assert_eq!(back.calendar_ids.get("cal1"), Some(&true));
    assert_eq!(back.event["uid"], "abc-123");
    assert_eq!(back.event["title"], "Standup");
    assert!(
        !json.contains("utcStart"),
        "fetch-time-only properties are not sent back: {json}"
    );
}

#[test]
fn calendar_event_query_expands_recurrences() {
    let body = br#"{
      "methodResponses": [
        ["CalendarEvent/query", {
          "queryState": "s2",
          "position": 0,
          "total": 2,
          "ids": ["ev1;2026-08-10T10:00:00", "ev1;2026-08-11T10:00:00"]
        }, "c0"],
        ["CalendarEvent/get", {
          "state": "s2",
          "list": [
            {"id": "ev1;2026-08-10T10:00:00", "utcStart": "2026-08-10T08:00:00Z", "title": "Standup"},
            {"id": "ev1;2026-08-11T10:00:00", "utcStart": "2026-08-11T08:00:00Z", "title": "Standup"}
          ],
          "notFound": []
        }, "c1"]
      ],
      "sessionState": "s2"
    }"#;

    let opts = JmapCalendarEventQueryOptions {
        filter: Some(JmapCalendarEventFilter {
            in_calendar: Some("cal1".into()),
            after: Some("2026-08-01T00:00:00".into()),
            before: Some("2026-09-01T00:00:00".into()),
            ..Default::default()
        }),
        sort: Some(vec![JmapCalendarEventSortComparator {
            property: JmapCalendarEventSortProperty::Start,
            is_ascending: Some(true),
        }]),
        limit: Some(50),
        expand_recurrences: true,
        properties: Some(vec!["title".into(), "utcStart".into()]),
        ..Default::default()
    };

    match run_calendar_event_query(&http_ok(body), opts) {
        (request, JmapCoroutineState::Complete(Ok(out))) => {
            assert!(request.contains(r#""expandRecurrences":true"#), "{request}");
            assert!(request.contains(r#""inCalendar":"cal1""#), "{request}");
            assert!(
                request.contains(r#""sort":[{"isAscending":true,"property":"start"}]"#),
                "{request}"
            );
            assert!(
                request.contains(
                    r##""#ids":{"name":"CalendarEvent/query","path":"/ids","resultOf":"c0"}"##
                ),
                "the get half back-references the query results: {request}"
            );

            assert_eq!(out.events.len(), 2);
            assert_eq!(out.total, Some(2));
            assert_eq!(out.position, 0);
            assert_eq!(out.query_state, "s2");
            assert_eq!(
                out.events[0].utc_start.as_deref(),
                Some("2026-08-10T08:00:00Z")
            );
            assert_eq!(
                out.events[1].utc_start.as_deref(),
                Some("2026-08-11T08:00:00Z")
            );
        }
        (_, other) => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn calendar_event_query_missing_get_response() {
    let body = br#"{
      "methodResponses": [
        ["CalendarEvent/query", {"queryState": "s2", "position": 0, "ids": []}, "c0"]
      ],
      "sessionState": "s2"
    }"#;

    match run_calendar_event_query(&http_ok(body), JmapCalendarEventQueryOptions::default()) {
        (_, JmapCoroutineState::Complete(Err(JmapCalendarEventQueryError::MissingGetResponse))) => {
        }
        (_, other) => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn calendar_event_query_method_error() {
    let body = br#"{
      "methodResponses": [
        ["error", {"type": "invalidArguments", "description": "unsupported filter"}, "c0"],
        ["CalendarEvent/get", {"state": "s2", "list": [], "notFound": []}, "c1"]
      ],
      "sessionState": "s2"
    }"#;

    match run_calendar_event_query(&http_ok(body), JmapCalendarEventQueryOptions::default()) {
        (_, JmapCoroutineState::Complete(Err(JmapCalendarEventQueryError::QueryMethod(err)))) => {
            assert!(err.to_string().contains("unsupported filter"), "{err}");
        }
        (_, other) => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn calendar_event_changes_ok() {
    let body = br#"{
      "methodResponses": [
        ["CalendarEvent/changes", {
          "oldState": "s1",
          "newState": "s2",
          "hasMoreChanges": true,
          "created": [],
          "updated": ["ev1"],
          "destroyed": []
        }, "c0"]
      ],
      "sessionState": "s2"
    }"#;

    match run_calendar_event_changes(&http_ok(body)) {
        (request, JmapCoroutineState::Complete(Ok(out))) => {
            assert!(request.contains(r#""maxChanges":42"#), "{request}");
            assert!(out.has_more_changes);
            assert_eq!(out.updated, vec!["ev1".to_string()]);
        }
        (_, other) => panic!("unexpected result: {other:?}"),
    }
}
