---
cairn: delta
change: bounded-frames-and-checked-inputs
---

## ADDED Requirements

### Requirement: Event source frames are bounded
`JmapEventSource` SHALL fail with `FrameTooLarge(max_frame_len)` once one SSE frame exceeds its `max_frame_len`, 64 KiB by default (`JMAP_DEFAULT_MAX_FRAME_LEN`) and settable after `new`, before the first resume. The size of a frame SHALL be the bytes of the lines since the last blank line, the line in progress included, comment lines and line ends aside: a frame that fits is never refused, however many frames or comment lines the stream carries. The count SHALL be taken on each decoded chunk before the SSE parser sees it, so what the parser holds stays within a small multiple of `max_frame_len`.

#### Scenario: A line that never ends
- GIVEN a `JmapEventSource` with the default `max_frame_len`
- WHEN the server streams chunks holding no line end, 64 KiB and one byte in all
- THEN the coroutine completes with `FrameTooLarge(65536)`

#### Scenario: Data lines with no blank line
- GIVEN a `JmapEventSource` whose `max_frame_len` is 64
- WHEN the server streams `data:` lines with no blank line, past 64 bytes in all
- THEN the coroutine completes with `FrameTooLarge(64)`

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

## MODIFIED Requirements

### Requirement: Responses are bounded
The second paragraph: `JmapEventSource` SHALL read its head through `Http11HeadersRead`, failing with `ReadHeaders(TooLarge)` past 64 KiB, and its chunks through `Http11ChunksReadStream`, failing with `ReadChunks(ChunkTooLarge)` on a chunk over its `max_frame_len`. The sentence leaving the SSE frames unbounded is gone.

## REMOVED Requirements
