---
cairn: log
change: set-results-and-request-errors
landed: 2026-11-13
---

# Set results are read whole, request errors keep their problem

A correction to the entry of the same day on bounded frames, then four findings of the family audit of 2026-11-13.

Correction: that change handed `max_frame_len` to the chunk reader, so one chunk of many small frames past 64 KiB failed a valid stream with `ChunkTooLarge(65536)`. A chunk is a transport unit: `JmapEventSource` gained `max_chunk_len` (rfc8620/event_source/subscribe.rs), 1 MiB by default (`JMAP_DEFAULT_MAX_CHUNK_LEN`), and `max_frame_len` bounds frames only. The count still runs on a whole chunk before the parser sees it, so a chunk holding a frame over the limit fails before the frames ahead of it are yielded; the stream is failing either way.

JMAP-01: `JmapVacationResponseSet` (rfc8621/vacation_response/set.rs) reads `notUpdated` and completes with the new `NotUpdated(JmapVacationResponseSetItemError)` when it names the singleton. `JmapClientStd::vacation_response_set` needed no change, it already returns the coroutine's error. **Breaking**: the new variant.

JMAP-04: `JmapEmailSubmissionSet` (rfc8621/email_submission/set.rs) reads the implicit `Email/set` response, found by name and call id, into the new `email_set` field, an `Option<Result<JmapEmailSetOutput, JmapMethodError>>`. A failed or unreadable implicit response is an `Err` inside the output, not an error of the coroutine: the submission went out, and "sent but not filed" must not read as "not sent". **Breaking** for a literal of `JmapEmailSubmissionSetOutput` only.

JMAP-09: the private response struct of `JmapSet` became the `pub(crate)` `SetResponse<T, E>` (rfc8620/set.rs), `newState` optional and nullable, and `JmapEmailSubmissionSet`, `JmapEmailSubmissionCancel`, `JmapEmailImport`, `JmapEmailCopy` and `JmapContactCardCopy` read through it, their five copies gone. `JmapEmailParse` reads `parsed: null` as empty. Which server omits `newState` on these five methods was not verified; the tolerance is the one `JmapSet` already had. Not breaking.

JMAP-10: `JmapRequestError` (rfc8620/error.rs) models the request-level errors of RFC 8620 §3.6.1 and `JmapSend` fails with the new `Request { status, error }` when a non-2xx body names a problem of the `urn:ietf:params:jmap:error:` namespace. The audit asked for every JSON problem body to read so; that was narrowed, since a server answering all its errors with `about:blank` problem details would turn every `HttpStatus(401)` a consumer matches on into a variant it does not know, with no compile error to say so. **Breaking**: the new variant, and `UnknownCapability`, `NotJson`, `NotRequest` and `Limit` gone from `JmapMethodError`, whose doc now cites §3.6.2. A consumer mapping `HttpStatus(status)` to its own error wants a second arm for `Request { status, .. }`.

Capabilities moved: responses ("Responses are bounded", "Event source frames are bounded" and "A missing new state is empty" modified; new: "A refused vacation update is an error", "A submission keeps the implicit Email/set answer", "Request errors keep the problem the server named").

Left open: the SetError types `JmapMethodError` still carries (`NotFound`, `InvalidPatch`, `WillDestroy`, `InvalidProperties`, `Singleton`), which RFC 8620 §3.6.2 does not list.

Verified, each test red before its fix: against the unfixed code the chunk of 2000 frames completed with `ReadChunks(ChunkTooLarge(65536))`, the refused vacation update completed `Ok`, the output of the submission carried no trace of the `notFound`, the five set-like responses failed with `ParseResponse(missing field newState)` (`invalid type: null, expected a string` for the cancel flow), `parsed: null` with `invalid type: null, expected a map`, and the 400 displayed `JMAP send failed: HTTP 400`. Tests: `chunk_of_small_frames_over_max_frame_len_streams_every_frame`, `a_refused_update_returns_an_error`, `problem_details_return_request_error` and `refusal_without_new_state_returns_ok` in the crate, the `email_submission_*`, `email_import_*`, `email_copy_*` and `email_parse_*` ones in tests/rfc8621.rs. `chunk_over_max_frame_len_returns_chunk_too_large`, which pinned the wrong chunk bound, became `chunk_over_max_chunk_len_returns_chunk_too_large`.
