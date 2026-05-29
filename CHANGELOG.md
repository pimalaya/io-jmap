# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added basic I/O-free coroutines.

- Added standard, blocking client.

- Added JMAP Event Source types and parser (`rfc8620::event_source`): `StateChange`, `TypeStates`, `parse_state_change()`, and `subscribe_url()` for composing the SSE endpoint URL from the session. Pairs with `io-http`'s `sse` module to drive RFC 8620 §7.2 push.

### Changed

- Unified all standard-shape coroutines under a single `JmapCoroutine` trait (in `crate::coroutine`) with associated `Output` and `Error`. `resume` now returns `JmapCoroutineState<Output, Error>` directly; the per-coroutine `Jmap*Result` enums are gone, replaced by small `Jmap*Ok` output structs. `JmapClientStd::run<C: JmapCoroutine>` drives any coroutine to completion. Exempt (kept as-is with their own result enum because they carry a `WantsRedirect` variant): `JmapSessionGet`, `JmapBlobDownload`, `JmapBlobUpload`. Internal helpers (`JmapSend`, `JmapGet<T>`, `JmapSet<T>`) keep their own result enums.

[unreleased]: https://github.com/pimalaya/io-jmap/compare/root..HEAD
