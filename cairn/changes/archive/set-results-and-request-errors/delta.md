---
cairn: delta
change: set-results-and-request-errors
---

## ADDED Requirements

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

## MODIFIED Requirements

### Requirement: Responses are bounded
The second paragraph: the event source chunks fail with `ReadChunks(ChunkTooLarge)` over `max_chunk_len`, 1 MiB by default (`JMAP_DEFAULT_MAX_CHUNK_LEN`), no longer over `max_frame_len`. A chunk is a transport unit, not a frame: its bound is its own, and one chunk MAY hold any number of frames.

### Requirement: Event source frames are bounded
What the parser holds stays within one chunk and a small multiple of `max_frame_len`; a chunk holding a frame over the limit fails whole, the frames ahead of it in that chunk included.

#### Scenario: One chunk of many small frames
- GIVEN a `JmapEventSource` with the default limits
- WHEN the server sends one chunk holding 2000 frames of 43 bytes, past 64 KiB in all
- THEN the coroutine yields the 2000 frames

### Requirement: A missing new state is empty
Extended to every method answering like a `Foo/set`: `JmapEmailSubmissionSet`, `JmapEmailSubmissionCancel`, `JmapEmailImport`, `JmapEmailCopy` and `JmapContactCardCopy` read their response through the one response type `JmapSet` reads. `JmapEmailParse` SHALL read a `parsed: null` (RFC 8621 §4.9) as an empty map, keeping `not_parsable` and `not_found`.

#### Scenario: Every submission refused
- GIVEN an `EmailSubmission/set` whose one create has no recipient
- WHEN the server answers `notCreated: {"c1": {"type": "noRecipients"}}` and no `newState`
- THEN the coroutine completes with an empty `new_state` and `not_created["c1"]` is `NoRecipients`

## REMOVED Requirements
