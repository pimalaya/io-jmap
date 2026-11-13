---
cairn: tasks
change: io-http-0-7
---

# Tasks

- [x] io-http 0.7 in Cargo.toml, `cargo update -p io-http`, one version in `cargo tree -i io-http`
- [x] `JmapEventSourceError::InvalidRequest`, from `to_http_11_vec` in `JmapEventSource::new`
- [x] `max_frame_len` handed to `SseFrameParser` as `max_line_len` and `max_event_len`, both failures read as `FrameTooLarge`, `FrameLen` removed
- [x] `data_lines_without_blank_line_return_frame_too_large` moved to the count of the parser; the other event source tests untouched
- [x] `Host` port checked against io-http 0.7
- [x] The note on io-http's trace log removed from the requests spec
- [x] CHANGELOG, spec, log
