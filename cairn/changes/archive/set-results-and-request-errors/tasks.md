---
cairn: tasks
change: set-results-and-request-errors
---

# Tasks

- [x] Tests failing first: one chunk of 2000 small frames, a refused vacation update, an implicit `Email/set` with a `notUpdated`, the five set-like responses without `newState` (absent or `null`), `parsed: null`, a 400 with `limit` problem details
- [x] `JmapEventSource::max_chunk_len`, `JMAP_DEFAULT_MAX_CHUNK_LEN`, handed to the chunk reader in place of `max_frame_len`
- [x] `JmapVacationResponseSetItemError`, `NotUpdated`, `notUpdated` read
- [x] `JmapEmailSubmissionSetOutput::email_set`, the implicit response found by name and call id
- [x] `SetResponse<T, E>` shared by `JmapSet` and the five coroutines; `parsed` optional
- [x] `JmapRequestError`, `JmapSendError::Request`, the four request-level variants out of `JmapMethodError`
- [x] Control tests: a failed implicit `Email/set` stays in the output, a problem outside the JMAP namespace stays `HttpStatus`
- [x] CHANGELOG, spec, log
- [ ] Decide whether the SetError types leave `JmapMethodError`
