---
cairn: log
change: bounded-frames-and-checked-inputs
landed: 2026-11-13
---

# Event source frames are bounded, a content type and a keyword are checked

Three findings of the family audit of 2026-11-13.

JMAP-03: the event source grew without bound on a frame that never ends, the gap the 2026-11-03 entry left open for io-http's next breaking release. io-jmap now bounds it itself, on io-http 0.6: `JmapEventSource::max_frame_len` (rfc8620/event_source/subscribe.rs), 64 KiB by default (`JMAP_DEFAULT_MAX_FRAME_LEN`), fails the stream with the new `FrameTooLarge`, and is handed to the chunk reader as `max_chunk_len`, which was 256 MiB. The parser's pending bytes being private, a private `FrameLen` counts each decoded chunk before the parser sees it: the bytes of the lines since the last blank line, comment lines and line ends aside. The smaller count, bytes handed over since the last yielded frame, was rejected: keep-alive comments add up to the limit with no frame in between, and a partial frame followed by a chunk of many small ones overshoots it with no frame over the limit. The coroutine ends in its terminal state, the refused bytes never reaching the parser. **Breaking**: `JmapEventSourceError` gained a variant, and a chunk over 64 KiB now fails where one under 256 MiB passed.

JMAP-12: `JmapBlobUpload::new` (rfc8620/blob_upload.rs) wrote its `content_type` into the request head unchecked, so a value holding CR LF added a header line or cut the head before `Authorization`. It now returns a `Result` and refuses a CR, LF or NUL with `InvalidContentType`. **Breaking**: the signature, and the new variant. Every in-family caller passes the constant `message/rfc822`.

JMAP-06: `JmapEmailPatch` (rfc8621/email/set.rs) built `keywords/<keyword>` and `mailboxIds/<id>` without JSON Pointer escaping. `pointer()` moved from the calendar patch to rfc8620/set.rs and the four arms go through it. Not breaking.

Capabilities moved: responses ("Responses are bounded" modified, new: "Event source frames are bounded"), requests (new file: "A blob content type stays in its header", "Email patch paths are JSON Pointers").

Left open: the count duplicates the line splitting of io-http's `SseFrameParser`; once that parser takes a bound, `max_frame_len` is handed over and `FrameLen` goes.

Verified, each test red before its fix: against the unfixed code, 80 KiB of one line and 80 KiB of `data:` lines were buffered with no error, a chunk announcing 65537 bytes yielded `WantsRead`, the content type `text/plain\r\nX-Injected: 1` wrote `X-Injected: 1` as its own header line between `Content-Type` and `Authorization`, and the keyword `todo/later` serialised as `keywords/todo/later`. The event source and upload tests were then tightened to the new variants (`line_that_never_ends_returns_frame_too_large`, `data_lines_without_blank_line_return_frame_too_large`, `chunk_over_max_frame_len_returns_chunk_too_large`, `content_type_with_cr_lf_or_nul_returns_error`), the first three failing again with the two limits forced to `usize::MAX`; `frames_and_comments_within_limit_keep_streaming` is the control, and `patch_paths_escape_keywords_and_mailbox_ids` the patch test.
