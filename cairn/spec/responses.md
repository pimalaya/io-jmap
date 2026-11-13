---
cairn: spec
capability: responses
status: current
---

# Responses

How io-jmap reads what a JMAP server answers over HTTP/1.1, through io-http's readers, their failures surfacing as a variant of each coroutine's own error.

### Requirement: Responses are bounded
Every request-response coroutine (`JmapSend` and the method coroutines built on it, `JmapSessionGet`, `JmapBlobUpload`, `JmapBlobDownload`) SHALL read its response through io-http's `Http11Send` with its default bounds: a response head over 64 KiB SHALL fail with `Send(HeadersTooLarge)`, a body over 256 MiB with `Send(BodyTooLarge)`, and a connection closed before the response ends with `Send(Eof)`.

`JmapEventSource` SHALL read its head through `Http11HeadersRead`, failing with `ReadHeaders(TooLarge)` past 64 KiB, and its chunks through `Http11ChunksReadStream`, failing with `ReadChunks(ChunkTooLarge)` on a chunk over its `max_frame_len`.

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

### Requirement: Blob downloads take their own limit
`JmapBlobDownload` SHALL fail with `Send(BodyTooLarge(max_len))` once the blob exceeds its `max_len`, 256 MiB by default and settable after `new`. `JmapClientStd::blob_download` SHALL apply the client's `max_blob_len`, same default.

#### Scenario: A blob over the limit
- GIVEN a `JmapBlobDownload` whose `max_len` is 4
- WHEN the server answers 200 with a 10-byte body
- THEN the coroutine completes with `Send(BodyTooLarge(4))`

### Requirement: Set errors keep what the server named
Every per-object `SetError` enum (`JmapMailboxSetItemError`, `JmapEmailSetItemError`, `JmapEmailImportItemError`, `JmapEmailCopyItemError`, `JmapIdentitySetItemError`, `JmapEmailSubmissionSetItemError`, `JmapAddressBookSetItemError`, `JmapContactCardSetItemError`, `JmapContactCardCopyItemError`, `JmapCalendarEventSetItemError`) SHALL read a type it does not model, or a modelled type whose other properties do not fit, as `Other { type_name, description }`, carrying the `type` and `description` the server sent, and SHALL display it as `JMAP <type>[: <description>]`. A `Foo/set` error that is no SetError object at all SHALL read as `Other` with an empty `type_name`.

`alreadyExists` SHALL carry the `existingId` the server MUST name (RFC 8620 §5.4) as `existing_id`, on `Email/import` (RFC 8621 §4.8), `Email/copy` and `ContactCard/copy`; one naming no `existingId` reads as `Other`.

#### Scenario: A duplicate import
- GIVEN a server forbidding duplicate emails
- WHEN `Email/import` answers `notCreated: {"m0": {"type": "alreadyExists", "existingId": "E1"}}`
- THEN `not_created["m0"]` is `AlreadyExists { existing_id: "E1", .. }`, displayed `JMAP alreadyExists (existing id E1)`

### Requirement: A missing new state is empty
`JmapSet` and `JmapVacationResponseSet` SHALL read a `newState` the server leaves out (Stalwart, on a call that changed nothing) or sends as `null` (Fastmail, on `VacationResponse/set`) as an empty `new_state`, rather than failing a call whose changes the server applied.

### Requirement: Method errors keep what the server named
`JmapMethodError` SHALL read a method error type it does not model as `Other { type_name, description }`, displayed `JMAP <type>[: <description>]`, and SHALL keep the `arguments` a server names on `invalidArguments`, displayed `JMAP invalidArguments [<arguments>]`. A method response named `error` whose arguments are no error object SHALL read as `Other` with an empty `type_name` and the parse failure as `description`.

### Requirement: A chained query keeps the query order
Every coroutine chaining a `Foo/query` into a back-referenced `Foo/get` (`JmapMailboxQuery`, `JmapEmailQuery`, `JmapEmailSubmissionQuery`, `JmapCalendarEventQuery`, `JmapContactCardQuery`) SHALL return the fetched objects in the order of the query `ids`, an object the query did not name last, whatever order the server lists them in.

#### Scenario: A server listing in its own order
- GIVEN `Mailbox/query` answering `ids: ["m3", "m1", "m2"]` and `Mailbox/get` listing `m1`, `m2`, `m3`
- WHEN `JmapMailboxQuery` completes
- THEN its `mailboxes` are `m3`, `m1`, `m2`
