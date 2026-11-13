---
cairn: change
id: bounded-frames-and-checked-inputs
status: landed
created: 2026-11-13
---

# Bound the event source frames, check what reaches a request

## Why

Three findings of the family audit of 2026-11-13 (JMAP-03, JMAP-12, JMAP-06), each a value io-jmap took on trust.

- The event source is the one long-lived connection of the crate, and the one reader with no bound: a server streaming a line that never ends, or `data:` lines with no blank line, grows the SSE parser by every chunk, each chunk allowed up to 256 MiB, until the watcher is killed. The spec recorded the gap as waiting for io-http's parser to grow a bound. A StateChange is a few hundred bytes.
- `JmapBlobUpload::new` wrote its `content_type` into the request head as given. A caller passing a media type read from a received message could carry a CR LF into the head: one more header line, or an early end of the head that turns the `Authorization` line into body bytes.
- `JmapEmailPatch` built `keywords/<keyword>` and `mailboxIds/<id>` without escaping the last token, while RFC 8621 §4.1.1 allows `/` and `~` in a keyword: `todo/later` was always refused with `invalidPatch`, and a keyword holding `~0` or `~1` was read as another keyword.

## What

- `JmapEventSource::max_frame_len`, a `pub` field defaulting to `JMAP_DEFAULT_MAX_FRAME_LEN` (64 KiB), and `JmapEventSourceError::FrameTooLarge`. io-http 0.6 exposes neither the bytes its `SseFrameParser` holds nor an error, so io-jmap counts each decoded chunk before the parser sees it: the bytes of the lines since the last blank line, comment lines and line ends aside. Counting the bytes handed over since the last yielded frame would be smaller, but refuses valid streams (keep-alive comments, a chunk of many small frames after a partial one).
- The chunk reader takes the same limit as its `max_chunk_len`, it buffering a whole chunk before yielding it.
- `JmapBlobUpload::new` returns a `Result` and refuses a CR, LF or NUL with `InvalidContentType`, the three bytes RFC 9110 §5.5 names invalid in a field value.
- `pointer()` moves from the calendar patch to rfc8620/set.rs and the four path-building arms of `JmapEmailPatch` go through it.

The frame count duplicates the line splitting of io-http's parser. Once io-http bounds its parser, the count goes and `max_frame_len` is handed over.
