# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Fixed `JmapEventSource` mis-routing the head reader's leftover bytes (the start of the chunked body that arrives in the same socket read as the HTTP head) straight into the SSE parser. Those bytes are chunk-encoded; the parser saw `<hex chunk size>\r\n` lines as unknown SSE fields and silently split multi-chunk field values at the size-header boundary, truncating `data:` payloads and breaking JSON decoding. Leftover bytes now go through `Http11ReadChunksStream` first.

### Added

- Added basic I/O-free coroutines.

- Added standard, blocking client.

- Added JMAP Event Source types and parser (`rfc8620::event_source`): `StateChange`, `TypeStates`, `parse_state_change()`, and `subscribe_url()` for composing the SSE endpoint URL from the session. Pairs with `io-http`'s `sse` module to drive RFC 8620 §7.2 push.

### Changed

- Unified all standard-shape coroutines under a single `JmapCoroutine` trait (in `crate::coroutine`) with associated `Output` and `Error`. `resume` now returns `JmapCoroutineState<Output, Error>` directly; the per-coroutine `Jmap*Result` enums are gone, replaced by small `Jmap*Ok` output structs. `JmapClientStd::run<C: JmapCoroutine>` drives any coroutine to completion. Exempt (kept as-is with their own result enum because they carry a `WantsRedirect` variant): `JmapSessionGet`, `JmapBlobDownload`, `JmapBlobUpload`. Internal helpers (`JmapSend`, `JmapGet<T>`, `JmapSet<T>`) keep their own result enums.

- Migrated the coroutine trait to the generator shape mirrored from io-http: associated `Yield` and `Return` types; `resume` returns a two-variant `JmapCoroutineState<Y, R>` (`Yielded(Y)` / `Complete(R)`); the standard I/O-only yield is `JmapYield` (`WantsRead` / `WantsWrite(Vec<u8>)`); per-coroutine `Jmap*Output` structs replace anonymous multi-field `Ok { … }` variants. The three redirect-capable coroutines (`JmapSessionGet`, `JmapBlobDownload`, `JmapBlobUpload`) now also implement `JmapCoroutine` with a shared `JmapRedirectYield` (adds `WantsRedirect { url, keep_alive, same_origin }`) instead of their bespoke result enums. `JmapClientStd::run<C, T, E>` is now generic over the coroutine and constrained to `Yield = JmapYield`; per-method loops handle the redirect-aware coroutines. `JmapEventSource` follows the same shape with its own `JmapEventSourceYield` (adds `Frame(StateChange)`).

[unreleased]: https://github.com/pimalaya/io-jmap/compare/root..HEAD
