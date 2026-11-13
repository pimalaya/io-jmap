---
cairn: delta
change: io-http-0-7
---

## ADDED Requirements

### Requirement: A header value cannot split a request head
No request SHALL be written whose header value holds a CR, LF or NUL, the `Authorization` value included: io-http checks the head it serialises. `JmapEventSource::new`, which serialises its request itself, SHALL return `InvalidRequest`; `JmapSend`, `JmapSessionGet`, `JmapBlobUpload` and `JmapBlobDownload` fail their first resume with `Send(InvalidRequest)`, before any write.

#### Scenario: A credential holding a line break
- GIVEN the authorization value `Bearer t\r\nX-Injected: 1`
- WHEN `JmapEventSource::new` builds its request
- THEN it returns `InvalidRequest(InvalidHeaderValue("Authorization"))`

## MODIFIED Requirements

### Requirement: Event source frames are bounded
`JmapEventSource` SHALL fail with `FrameTooLarge(max_frame_len)` once one line of an SSE frame, or the data of one frame, exceeds its `max_frame_len`. The limit SHALL be handed to io-http's `SseFrameParser` as its `max_line_len` and its `max_event_len`, whose `LineTooLarge` and `EventTooLarge` both read as `FrameTooLarge`; io-jmap counts nothing itself. A line counts without its line end, the line in progress and a comment line included. The data of a frame counts as the caller reads it, the values of its `data` lines joined by a LF: the field names and the other fields (`event`, `id`, `retry`, unknown ones) are bounded as lines and do not add up. The count on each decoded chunk before the parser sees it is gone, and with it the failure of a whole chunk, the frames ahead of the oversized one included.

#### Scenario: Data lines with no blank line
- GIVEN a `JmapEventSource` whose `max_frame_len` is 64
- WHEN the server streams `data:` lines with no blank line, whose values joined by a LF reach 64 bytes, then one more `data:` line
- THEN the coroutine completes with `FrameTooLarge(64)`

### Requirement: Secrets stay out of Debug and logs
The sentence saying io-http 0.6 traces the whole request, its URL included, under its own target, is removed: io-http 0.7 logs a request with the userinfo and the query values of its URL redacted.

## REMOVED Requirements
