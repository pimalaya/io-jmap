---
cairn: spec
capability: responses
status: current
---

# Responses

How io-jmap reads what a JMAP server answers over HTTP/1.1, through io-http's readers, their failures surfacing as a variant of each coroutine's own error.

### Requirement: Responses are bounded
Every request-response coroutine (`JmapSend` and the method coroutines built on it, `JmapSessionGet`, `JmapBlobUpload`, `JmapBlobDownload`) SHALL read its response through io-http's `Http11Send` with its default bounds: a response head over 64 KiB SHALL fail with `Send(HeadersTooLarge)`, a body over 256 MiB with `Send(BodyTooLarge)`, and a connection closed before the response ends with `Send(Eof)`.

`JmapEventSource` SHALL read its head through `Http11HeadersRead`, failing with `ReadHeaders(TooLarge)` past 64 KiB, and its chunks through `Http11ChunksReadStream`, failing with `ReadChunks(ChunkTooLarge)` on a chunk over its `max_chunk_len`, 1 MiB by default (`JMAP_DEFAULT_MAX_CHUNK_LEN`) and settable after `new`, before the first resume. A chunk is a transport unit, not a frame: its bound is its own, and one chunk MAY hold any number of frames.

### Requirement: Method responses are decoded once
`JmapResponse` SHALL keep the result of each method response as the JSON text the server sent (`Box<RawValue>`), and every coroutine SHALL decode that text once, straight into its typed response: no generic JSON tree is built of a response, so a property the library does not model costs no allocation, and what a response holds on the heap follows its wire size, not the number of values in it. The per-object errors of a `Foo/set` stay generic values until their wrapper types them, being few and small.

#### Scenario: Emails carrying properties the library does not model
- GIVEN an `Email/get` response of 1000 emails, each an id and 20 unmodelled properties
- WHEN `JmapEmailGet` decodes it
- THEN it allocates fewer than 2000 times, one string per id and the buffers, where a JSON tree took over 46000

### Requirement: Event source frames are bounded
`JmapEventSource` SHALL fail with `FrameTooLarge(max_frame_len)` once one line of an SSE frame, or the data of one frame, exceeds its `max_frame_len`, 64 KiB by default (`JMAP_DEFAULT_MAX_FRAME_LEN`) and settable after `new`, before the first resume. The limit SHALL be handed to io-http's `SseFrameParser` as its `max_line_len` and its `max_event_len`, whose `LineTooLarge` and `EventTooLarge` both read as `FrameTooLarge`; io-jmap counts nothing itself. A line counts without its line end, the line in progress and a comment line included. The data of a frame counts as the caller reads it, the values of its `data` lines joined by a LF: the field names and the other fields (`event`, `id`, `retry`, unknown ones) are bounded as lines and do not add up, being replaced or dropped, not kept. A frame that fits is never refused, however many frames or comment lines the stream carries, and what the parser holds stays within one chunk and a small multiple of `max_frame_len`.

#### Scenario: One chunk of many small frames
- GIVEN a `JmapEventSource` with the default limits
- WHEN the server sends one chunk holding 2000 frames of 43 bytes, past 64 KiB in all
- THEN the coroutine yields the 2000 frames

#### Scenario: A line that never ends
- GIVEN a `JmapEventSource` with the default `max_frame_len`
- WHEN the server streams chunks holding no line end, 64 KiB and one byte in all
- THEN the coroutine completes with `FrameTooLarge(65536)`

#### Scenario: Data lines with no blank line
- GIVEN a `JmapEventSource` whose `max_frame_len` is 64
- WHEN the server streams `data:` lines with no blank line, whose values joined by a LF reach 64 bytes, then one more `data:` line
- THEN the coroutine completes with `FrameTooLarge(64)`

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

### Requirement: Blob downloads take their own limit
`JmapBlobDownload` SHALL fail with `Send(BodyTooLarge(max_len))` once the blob exceeds its `max_len`, 256 MiB by default and settable after `new`. `JmapClientStd::blob_download` SHALL apply the client's `max_blob_len`, same default.

#### Scenario: A blob over the limit
- GIVEN a `JmapBlobDownload` whose `max_len` is 4
- WHEN the server answers 200 with a 10-byte body
- THEN the coroutine completes with `Send(BodyTooLarge(4))`

### Requirement: Set errors keep what the server named
Every per-object `SetError` enum (`JmapMailboxSetItemError`, `JmapEmailSetItemError`, `JmapEmailImportItemError`, `JmapEmailCopyItemError`, `JmapIdentitySetItemError`, `JmapEmailSubmissionSetItemError`, `JmapAddressBookSetItemError`, `JmapCalendarSetItemError`, `JmapContactCardSetItemError`, `JmapContactCardCopyItemError`, `JmapCalendarEventSetItemError`) SHALL read a type it does not model, or a modelled type whose other properties do not fit, as `Other { type_name, description }`, carrying the `type` and `description` the server sent, and SHALL display it as `JMAP <type>[: <description>]`. A `Foo/set` error that is no SetError object at all SHALL read as `Other` with an empty `type_name`.

`JmapEmailSetItemError`, `JmapEmailCopyItemError`, `JmapEmailImportItemError` and `JmapMailboxSetItemError` SHALL model the standard types `forbidden`, `overQuota`, `tooLarge` and `rateLimit` (RFC 8620 §5.3), each with its `description`. `JmapEmailSubmissionSetItemError` SHALL carry the `invalidRecipients` list of `invalidRecipients` as `invalid_recipients`, `[]` when absent, and the `maxRecipients` of `tooManyRecipients` as `max_recipients`, `None` when absent (RFC 8621 §7.5).

`alreadyExists` SHALL carry the `existingId` the server MUST name (RFC 8620 §5.4) as `existing_id`, on `Email/import` (RFC 8621 §4.8), `Email/copy` and `ContactCard/copy`; one naming no `existingId` reads as `Other`.

#### Scenario: A duplicate import
- GIVEN a server forbidding duplicate emails
- WHEN `Email/import` answers `notCreated: {"m0": {"type": "alreadyExists", "existingId": "E1"}}`
- THEN `not_created["m0"]` is `AlreadyExists { existing_id: "E1", .. }`, displayed `JMAP alreadyExists (existing id E1)`

#### Scenario: A refused keyword update
- WHEN `Email/set` answers `notUpdated: {"e1": {"type": "forbidden", "description": "read-only mailbox"}}`
- THEN `not_updated["e1"]` is `Forbidden { description: Some("read-only mailbox") }`, displayed `JMAP forbidden: read-only mailbox`

#### Scenario: Two refused recipients
- WHEN `EmailSubmission/set` answers `notCreated: {"s0": {"type": "invalidRecipients", "invalidRecipients": ["a@x", "b@y"]}}`
- THEN `not_created["s0"]` is `InvalidRecipients { invalid_recipients: ["a@x", "b@y"], .. }`

### Requirement: A missing new state is empty
`JmapSet` and `JmapVacationResponseSet` SHALL read a `newState` the server leaves out (Stalwart, on a call that changed nothing) or sends as `null` (Fastmail, on `VacationResponse/set`) as an empty `new_state`, rather than failing a call whose changes the server applied.

So SHALL every method answering like a `Foo/set`: `JmapEmailSubmissionSet`, `JmapEmailSubmissionCancel`, `JmapEmailImport`, `JmapEmailCopy` and `JmapContactCardCopy` read their response through the one response type `JmapSet` reads, a call that changed nothing being the one whose `notCreated` or `notUpdated` the caller needs. `JmapEmailParse` SHALL read a `parsed: null` (RFC 8621 §4.9, no blob parsed) as an empty map, keeping `not_parsable` and `not_found`.

#### Scenario: Every submission refused
- GIVEN an `EmailSubmission/set` whose one create has no recipient
- WHEN the server answers `notCreated: {"c1": {"type": "noRecipients"}}` and no `newState`
- THEN the coroutine completes with an empty `new_state` and `not_created["c1"]` is `NoRecipients`

### Requirement: A refused vacation update is an error
`JmapVacationResponseSet` SHALL complete with `NotUpdated(JmapVacationResponseSetItemError)` when the response names the singleton under `notUpdated` (RFC 8620 §5.3), the update having one target and a success echoing nothing being otherwise indistinguishable from a refusal. The item error SHALL model `invalidProperties` and `invalidPatch` and keep any other type as `Other`, as every per-object SetError enum does. `JmapClientStd::vacation_response_set` SHALL return it as `VacationResponseSet(NotUpdated(..))`.

#### Scenario: A date the server refuses
- GIVEN an update whose `toDate` the server does not accept
- WHEN it answers `notUpdated: {"singleton": {"type": "invalidProperties", "properties": ["toDate"]}}`
- THEN the coroutine completes with `NotUpdated(InvalidProperties { properties: ["toDate"], .. })`, displayed `JMAP VacationResponse/set failed: JMAP invalidProperties [toDate]`

### Requirement: A submission keeps the implicit Email/set answer
`JmapEmailSubmissionSet` SHALL read, after the `EmailSubmission/set` response, the response to the `Email/set` the server runs for `onSuccessUpdateEmail` and `onSuccessDestroyEmail` (RFC 8621 §7.5), found by its name (`Email/set`, or `error`) and the call id of the submission, not by position, and SHALL hand it over as `email_set`: `None` when the server sent none, `Some(Ok(JmapEmailSetOutput))` when it answered, `Some(Err(JmapMethodError))` when that call failed whole or its response could not be read. A failed implicit call SHALL NOT fail the coroutine: the submissions in `created` were sent, and an error would read as nothing sent.

#### Scenario: Sent but not filed
- GIVEN a submission with `onSuccessUpdateEmail` moving the email to a mailbox that is gone
- WHEN the server answers `EmailSubmission/set` with `created` then `Email/set` with `notUpdated: {"e1": {"type": "notFound"}}`
- THEN the coroutine completes with the created submission and `email_set` is `Some(Ok(..))` whose `not_updated["e1"]` is `NotFound`

### Requirement: Request errors keep the problem the server named
`JmapSend`, and every method coroutine through its `Send` variant, SHALL read the body of a non-2xx answer as problem details (RFC 7807) and, when its `type` is in the `urn:ietf:params:jmap:error:` namespace, fail with `Request { status, error }`, `error` being a `JmapRequestError`: `UnknownCapability`, `NotJson` (`notJSON`), `NotRequest`, `Limit { limit }` (RFC 8620 §3.6.1), or `Other { type_name }` for a type of the namespace it does not model, each keeping the `detail`. Any other non-2xx answer (no body, a body that is no problem, a problem of another namespace such as `about:blank`) SHALL fail with `HttpStatus(status)` as before, so a caller keyed on a status keeps seeing it. `JmapMethodError` SHALL NOT model the four request-level types, which never arrive as a method response.

#### Scenario: Too many calls
- GIVEN a request with more calls than the server takes
- WHEN it answers 400 with `{"type": "urn:ietf:params:jmap:error:limit", "limit": "maxCallsInRequest", "detail": "Too many calls"}`
- THEN the coroutine completes with `Request { status: 400, error: Limit { limit: "maxCallsInRequest", .. } }`, displayed `JMAP send failed: HTTP 400: JMAP limit (maxCallsInRequest): Too many calls`

### Requirement: Method errors keep what the server named
`JmapMethodError` SHALL read a method error type it does not model as `Other { type_name, description }`, displayed `JMAP <type>[: <description>]`, and SHALL keep the `arguments` a server names on `invalidArguments`, displayed `JMAP invalidArguments [<arguments>]`. A method response named `error` whose arguments are no error object SHALL read as `Other` with an empty `type_name` and the parse failure as `description`.

### Requirement: A chained query keeps the query order
Every coroutine chaining a `Foo/query` into a back-referenced `Foo/get` (`JmapMailboxQuery`, `JmapEmailQuery`, `JmapEmailSubmissionQuery`, `JmapCalendarEventQuery`, `JmapContactCardQuery`) SHALL return the fetched objects in the order of the query `ids`, an object the query did not name last, whatever order the server lists them in, and SHALL return the `/get`'s `notFound` as `not_found`, in the order the server sent it.

#### Scenario: A server listing in its own order
- GIVEN `Mailbox/query` answering `ids: ["m3", "m1", "m2"]` and `Mailbox/get` listing `m1`, `m2`, `m3`
- WHEN `JmapMailboxQuery` completes
- THEN its `mailboxes` are `m3`, `m1`, `m2`

#### Scenario: An email gone between the query and the get
- GIVEN `Email/query` answering `ids: ["e1", "e2"]` and `Email/get` listing `e1` with `notFound: ["e2"]`
- WHEN `JmapEmailQuery` completes
- THEN its `emails` hold `e1` and its `not_found` is `["e2"]`

### Requirement: An object that does not deserialize is reported, not fatal
`JmapEmailGet`, `JmapEmailQuery`, `JmapMailboxGet` and `JmapMailboxQuery` SHALL read each object of the `/get` `list` on its own. An object that does not deserialize SHALL be returned in `unparsed` as a `JmapUnparsed { id, error }`, `id` the object's `id` when it is a string and `error` the message of the deserialization failure, the other objects returned as usual. A response whose `list` is no array SHALL still fail whole, with `ParseGetResponse` from a chained query and `Get(ParseResponse)` from `JmapEmailGet` and `JmapMailboxGet`.

#### Scenario: One malformed email among two
- GIVEN an `Email/get` answering `list: [{"id": "e1", …}, {"id": "e2", "receivedAt": 42}]`
- WHEN `JmapEmailGet` completes
- THEN its `emails` hold `e1` and its `unparsed` holds `{id: Some("e2"), error: …}`
