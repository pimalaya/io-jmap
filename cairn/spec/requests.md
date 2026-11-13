---
cairn: spec
capability: requests
status: current
---

# Requests

What io-jmap puts on the wire: the values it refuses or escapes before they reach a request head or a method call, and what it writes there itself.

### Requirement: A blob content type stays in its header
`JmapBlobUpload::new` SHALL refuse a `content_type` holding a CR, LF or NUL with `InvalidContentType`, before any request is built, these bytes being invalid in a field value (RFC 9110 §5.5). `JmapClientStd::blob_upload` SHALL surface it as `BlobUpload(InvalidContentType)`.

#### Scenario: A content type carrying a header line
- GIVEN the content type `text/plain\r\nX-Injected: 1`
- WHEN `JmapBlobUpload::new` is called
- THEN it returns `InvalidContentType` and no request is written

### Requirement: Email patch paths are JSON Pointers
`JmapEmailPatch` SHALL write the keyword of `SetKeyword` and `UnsetKeyword` and the mailbox id of `AddToMailbox` and `RemoveFromMailbox` as one JSON Pointer token (RFC 6901 §3), `~` as `~0` and `/` as `~1`, the way `JmapCalendarEventPatch::diff` writes a recurrence id, RFC 8621 §4.1.1 allowing both characters in a keyword.

#### Scenario: A keyword holding a slash
- GIVEN the patch `set_keyword("todo/later")`
- WHEN it is serialised
- THEN its path is `keywords/todo~1later`

### Requirement: The Host header comes from the URL
No coroutine SHALL set `Host` by hand: `JmapSend`, `JmapSessionGet`, `JmapBlobUpload`, `JmapBlobDownload` and `JmapEventSource` leave it to io-http, which writes the host of the request URL and its port when it is not the scheme default (RFC 9110 §7.2).

#### Scenario: A server on another port
- GIVEN the API URL `https://api.example.com:8443/jmap/`
- WHEN `JmapSend` writes its request
- THEN the head holds `host: api.example.com:8443`

### Requirement: A method is addressed to a named account
`JmapSession::account_for(capability)` SHALL return the primary account id the session names for the capability, and `JmapSessionAccountError(capability)` when it names none or an empty one. Every method coroutine built from a session SHALL take its `accountId` from it and fail in `new` with the `Account` variant of its own error, so that no request carries an empty `accountId`. `VacationResponse/get` and `VacationResponse/set` SHALL ask for the vacation response capability, then for the mail one.

Only the primary account of a capability is reachable through these coroutines: none takes an account id of its own. A caller addressing another account edits `primary_accounts` on its copy of the session.

#### Scenario: A session without a mail account
- GIVEN a session whose `primaryAccounts` is empty
- WHEN `JmapEmailGet::new` is called
- THEN it returns `Account(JmapSessionAccountError("urn:ietf:params:jmap:mail"))` and no request is built

### Requirement: An Email create omits what is unset
`JmapEmailSetArgs` SHALL serialise the emails under `create` without any member that is `null`, at every depth (body parts and addresses included): RFC 8620 §5.3 wants a server-set property omitted, and RFC 8621 §4.6 wants `charset` and `size` omitted from a body part naming a `partId`. `JmapEmail` itself keeps writing its unset properties as `null` wherever else it is serialised.

#### Scenario: A draft with a subject and a text part
- GIVEN a create whose email sets `mailboxIds`, `subject`, `from` and one `textBody` part with a `partId` and a `type`
- WHEN the arguments are serialised
- THEN the created object holds these four members only, the address without `name` and the part without `blobId`, `size` or `charset`

### Requirement: An event source subscription can resume
`JmapEventSource::new` SHALL send the `last_event_id` of its `JmapEventSourceOptions` as the `Last-Event-ID` header (RFC 8620 §7.3), refusing one holding a CR, LF or NUL with `InvalidLastEventId`. `JmapEventSource::subscribe_url` SHALL subscribe to `*` when `types` is empty, the value RFC 8620 §7.3 defines for every type.

#### Scenario: Reconnecting after a state event
- GIVEN the id `evt-1` yielded with the last frame of a closed stream
- WHEN a new `JmapEventSource` is built with `last_event_id: Some("evt-1")`
- THEN its request head holds `Last-Event-ID: evt-1`
