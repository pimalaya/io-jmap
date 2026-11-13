---
cairn: change
id: set-results-and-request-errors
status: landed
created: 2026-11-13
---

# Read what a set answers, and why a request was refused

## Why

A correction to bounded-frames-and-checked-inputs, then four findings of the family audit of 2026-11-13 (JMAP-01, JMAP-04, JMAP-09, JMAP-10), each an answer the server gave and io-jmap dropped.

- The event source handed its `max_frame_len` to the chunk reader, so a valid stream whose transport coalesces more than 64 KiB of small frames into one chunk failed with `ChunkTooLarge`. A chunk is a transport unit, not a frame.
- `VacationResponse/set` never read `notUpdated`: a refused update completed `Ok` with `updated: None`, which is also what a plain success gives, so the caller was told the auto-reply was set when it was not.
- `EmailSubmission/set` read the first method response only. With `onSuccessUpdateEmail` or `onSuccessDestroyEmail` the server runs one implicit `Email/set` and answers it next (RFC 8621 §7.5); a `notUpdated` there was lost, the mail sent, the draft left in Drafts, and success reported.
- Five hand-rolled set responses (`EmailSubmission/set`, its cancel flow, `Email/import`, `Email/copy`, `ContactCard/copy`) still required `newState`, which the generic `JmapSet` tolerates missing since a call that changed nothing may omit it: exactly the call whose `notCreated` the caller needs, lost to `ParseResponse`. `Email/parse` refused the `parsed: null` RFC 8621 §4.9 allows, losing `notParsable` and `notFound` the same way.
- A request the server refuses whole answers 400 with problem details (RFC 8620 §3.6.1). `JmapSend` kept the status and dropped the body, so `limit: maxCallsInRequest` could not be told from `maxSizeRequest`, while `JmapMethodError` modelled the four request-level types, which never arrive as a method response.

## What

- `JmapEventSource::max_chunk_len`, a `pub` field defaulting to `JMAP_DEFAULT_MAX_CHUNK_LEN` (1 MiB), handed to the chunk reader; `max_frame_len` bounds frames only.
- `JmapVacationResponseSetItemError` and `JmapVacationResponseSetError::NotUpdated`: the coroutine fails when `notUpdated` names the singleton. An error rather than an output field, the update having one target.
- `JmapEmailSubmissionSetOutput::email_set`, an `Option<Result<JmapEmailSetOutput, JmapMethodError>>` read from the response named `Email/set` or `error` carrying the submission's call id. It stays in the output, never an error of the coroutine: the mail was sent, and a caller retrying on `Err` would send it twice.
- One `pub(crate)` `SetResponse<T, E>` in rfc8620/set.rs, `newState` optional and nullable, read by `JmapSet` and by the five coroutines, whose private response structs go. `parsed` becomes optional.
- `JmapRequestError` (`UnknownCapability`, `NotJson`, `NotRequest`, `Limit`, `Other`) and `JmapSendError::Request { status, error }`, the four variants leaving `JmapMethodError`. Only a problem of the `urn:ietf:params:jmap:error:` namespace reads as `Request`: any other non-2xx answer stays `HttpStatus`, so a consumer keyed on 401 or 412 is not silently bypassed by a server answering every error with `about:blank` problem details.

## Not in this change

`JmapMethodError` still carries SetError types (`NotFound`, `InvalidPatch`, `WillDestroy`, `InvalidProperties`, `Singleton`) that RFC 8620 §3.6.2 does not list as method errors. They are harmless and removing them breaks more than this change needs.
