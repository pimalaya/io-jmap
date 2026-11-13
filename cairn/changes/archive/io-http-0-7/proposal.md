---
cairn: change
id: io-http-0-7
status: landed
created: 2026-11-13
---

# Move to io-http 0.7 and hand it the frame bound

## Why

io-http 0.7.0, published 2026-11-13, checks the request head it serialises, bounds its SSE parser and redacts what it logs. Three things io-jmap carried only because io-http 0.6 did not do them can go or be corrected:

- `JmapEventSource` counted frame bytes itself, in a private `FrameLen` that split lines a second time ahead of io-http's `SseFrameParser`, whose pending bytes had no bound. The entry bounded-frames-and-checked-inputs left it open: once the parser takes a bound, `max_frame_len` is handed over and `FrameLen` goes.
- The spec said a token in a query still reached the trace log through io-http. It no longer does.
- `to_http_11_vec` returns a `Result`, and the parser's `Return` is a `Result`: the event source, the one coroutine serialising its request and driving the parser itself, does not build as it is.

## What

- io-http 0.7 in Cargo.toml and Cargo.lock, one version in the graph.
- `JmapEventSourceError::InvalidRequest(HttpRequestError)`, returned by `JmapEventSource::new` when the head cannot be serialised. `InvalidLastEventId` stays, checked first.
- `max_frame_len` handed to the parser as `max_line_len` and `max_event_len`; `LineTooLarge` and `EventTooLarge` both read as `FrameTooLarge`. `FrameLen` is removed.
- The note on io-http's trace log leaves the requests spec.

## What differs

The two counts are not the same. `FrameLen` counted the raw bytes of every field line since the last blank line, names included. The parser bounds each line, then the data of the frame as the caller reads it (the values of the `data` lines joined by a LF). So the names (`data: ` is 6 bytes a line) and the `event`, `id`, `retry` and unknown lines no longer add up: they are bounded as lines, and replaced or dropped, not kept. The new count is never above the old one, so no stream accepted before is refused now, and what the parser holds is bounded as before. A comment line was and stays bounded as a line. The frames ahead of an oversized one in the same chunk are now yielded before the failure, the count no longer running on a whole chunk first.

`max_frame_len` was never released with the old count.

## Not in this change

The `Host` header: checked, io-http 0.7 still writes it from the URL with a port that is not the default, the five `request_host_keeps_the_port` tests pass untouched.
