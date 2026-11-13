---
cairn: tasks
change: bounded-frames-and-checked-inputs
---

# Tasks

- [x] Tests failing first: a line that never ends, data lines with no blank line, a chunk announcing 64 KiB and one byte, a content type carrying a header line, a keyword holding `/` and `~`
- [x] `JmapEventSource::max_frame_len`, `JMAP_DEFAULT_MAX_FRAME_LEN`, `FrameTooLarge`, the frame count ahead of the SSE parser
- [x] Hand `max_frame_len` to the chunk reader as `max_chunk_len`
- [x] Control test: comment lines and whole frames past the limit in all keep streaming
- [x] `JmapBlobUpload::new` returns a `Result`, `InvalidContentType`
- [x] Move `pointer()` to rfc8620/set.rs, escape the keyword and the mailbox id in `JmapEmailPatch`
- [x] CHANGELOG, spec, log
- [ ] Hand `max_frame_len` to io-http's `SseFrameParser` and drop the local count, once io-http bounds its parser
