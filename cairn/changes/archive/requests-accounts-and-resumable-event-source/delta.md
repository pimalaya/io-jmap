---
cairn: delta
change: requests-accounts-and-resumable-event-source
---

## ADDED Requirements

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

### Requirement: Event source frames carry their id
`JmapEventSource` SHALL yield each StateChange as `Frame { change, id }`, `id` being the last event id the server set (RFC 8620 §7.3), kept across frames as the SSE parser keeps it, for the caller to hand back as `last_event_id` on its next connection. It SHALL read as a StateChange a frame naming the `state` event or no event, and SHALL skip a frame naming any other (the `ping` of RFC 8620 §7.3 among them) without yielding or failing; a frame it reads whose data is no StateChange still fails with `DecodeFrame`.

#### Scenario: An event the client does not know
- GIVEN a stream sending `event: other` with the data `hello`
- WHEN the frame is read
- THEN the coroutine asks for more bytes

### Requirement: Session discovery follows a redirect that stays put
`JmapSessionGet` SHALL yield every 3xx answer as `WantsRedirect`, following none. `JmapClientStd::session_get` SHALL follow it, on the same stream and with the same credentials, when the redirect keeps the scheme, host and port and the server keeps the connection, five times at most, and SHALL fail with `UnexpectedRedirect(url)` in every other case: another origin, a closed connection, a sixth redirect. `JmapClientStd::blob_upload` and `blob_download` follow none.

#### Scenario: A well-known URL answering with a redirect
- GIVEN a server answering `GET /.well-known/jmap` with a 301 to `/jmap/session` and keeping the connection
- WHEN `session_get` runs on the bare authority
- THEN it sends `GET /jmap/session` on the same stream and returns that session

## MODIFIED Requirements

## REMOVED Requirements
