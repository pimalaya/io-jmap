# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Fixed `JmapEventSource` mis-routing the head reader's leftover bytes (the start of the chunked body that arrives in the same socket read as the HTTP head) straight into the SSE parser. Those bytes are chunk-encoded; the parser saw `<hex chunk size>\r\n` lines as unknown SSE fields and silently split multi-chunk field values at the size-header boundary, truncating `data:` payloads and breaking JSON decoding. Leftover bytes now go through `Http11ReadChunksStream` first.

- Fixed cooperative shutdown for long-lived `JmapClientStd::watch_mailbox` callers: `JmapClientStd::connect` now sets a 5s per-read timeout on the underlying `StreamStd`, mirroring `io-imap`'s pattern. The watch driver already treated `WouldBlock` / `TimedOut` as "no new bytes", but without a timeout the SSE socket blocked indefinitely between push frames, so a Ctrl+C-driven shutdown atomic could not be polled until the server sent its next ping/state. The timeout is per-read (not per-operation), so non-watch HTTP responses remain unaffected as long as TCP packets keep arriving.

### Added

- Added basic I/O-free coroutines.

- Added standard, blocking client.

- Added JMAP Event Source types and parser (`rfc8620::event_source`): `StateChange`, `TypeStates`, `parse_state_change()`, and `subscribe_url()` for composing the SSE endpoint URL from the session. Pairs with `io-http`'s `sse` module to drive RFC 8620 §7.2 push.

### Changed

- Unified all standard-shape coroutines under a single `JmapCoroutine` trait (in `crate::coroutine`) with associated `Output` and `Error`. `resume` now returns `JmapCoroutineState<Output, Error>` directly; the per-coroutine `Jmap*Result` enums are gone, replaced by small `Jmap*Ok` output structs. `JmapClientStd::run<C: JmapCoroutine>` drives any coroutine to completion. Exempt (kept as-is with their own result enum because they carry a `WantsRedirect` variant): `JmapSessionGet`, `JmapBlobDownload`, `JmapBlobUpload`. Internal helpers (`JmapSend`, `JmapGet<T>`, `JmapSet<T>`) keep their own result enums.

- Migrated the coroutine trait to the generator shape mirrored from io-http: associated `Yield` and `Return` types; `resume` returns a two-variant `JmapCoroutineState<Y, R>` (`Yielded(Y)` / `Complete(R)`); the standard I/O-only yield is `JmapYield` (`WantsRead` / `WantsWrite(Vec<u8>)`); per-coroutine `Jmap*Output` structs replace anonymous multi-field `Ok { … }` variants. The three redirect-capable coroutines (`JmapSessionGet`, `JmapBlobDownload`, `JmapBlobUpload`) now also implement `JmapCoroutine` with a shared `JmapRedirectYield` (adds `WantsRedirect { url, keep_alive, same_origin }`) instead of their bespoke result enums. `JmapClientStd::run<C, T, E>` is now generic over the coroutine and constrained to `Yield = JmapYield`; per-method loops handle the redirect-aware coroutines. `JmapEventSource` follows the same shape with its own `JmapEventSourceYield` (adds `Frame(StateChange)`).

- Aligned every coroutine with the canonical io-imap / io-smtp / io-http template: normalized error messages to the `"JMAP <Method> failed: <detail>"` prefix; added a `jmap_try!` macro in `crate::coroutine` mirroring `imap_try!` / `http_try!`, plus `From<HttpSendYield> for JmapRedirectYield` so wrapping coroutines collapse their inner `match send.resume(...)` boilerplate to one line. Every coroutine now wraps its inner state in a private `State` enum with a `fmt::Display` impl and traces it on entry to `resume` for consistent low-level logs. Each module gained a runnable `# Example` block in its `//!` doc, the file layout is now strictly top-down (error → output → coroutine struct → impl → State → tests), and `rfc8620/{session_get, send, blob_download, blob_upload, get, set, query, changes, query_changes}` ship a canonical 5-test unit suite (success, HTTP error, redirect/parse error, parse failure, method-specific edge case) with shared `expect_*` helpers.

- Renamed the kebab-case `rfc8620/` file names to snake_case to drop the `#[path = "…"]` indirection: `blob-download.rs` → `blob_download.rs`, `blob-upload.rs` → `blob_upload.rs`, `query-changes.rs` → `query_changes.rs`, `session-get.rs` → `session_get.rs`. Public module paths (`rfc8620::blob_download`, etc.) are unchanged.

- Split `rfc8621/` into per-domain folders: `email/`, `email_submission/`, `identity/`, `mailbox/`, `thread/`, `vacation_response/`. Each folder bundles a private `types.rs` (re-exported via `#[doc(inline)] pub use types::*;`) and one `pub mod` per JMAP method. Public paths move from `rfc8621::email_get::JmapEmailGet` to `rfc8621::email::get::JmapEmailGet`; type re-exports keep `rfc8621::email::Email` working. Capability URN constants moved with their domain: `SUBMISSION_CAPABILITY` now lives in `rfc8621::email_submission`, `VACATION_RESPONSE_CAPABILITY` in `rfc8621::vacation_response`, `MAIL_CAPABILITY` at the `rfc8621` root; `CORE_CAPABILITY` at the `rfc8620` root.

- Consolidated `rfc8620/` data types into a single private `rfc8620/types.rs` re-exported via `#[doc(inline)] pub use types::*;`, leaving the directory with only coroutine files plus a `coroutine.rs` for shared coroutine plumbing (currently `JmapRedirectYield`). `JmapMethodError`, `SetError`, `Filter` / `FilterOperator` / `FilterOperatorKind`, `JmapSession` / `JmapAccountInfo`, `ResultReference`, `JmapRequest` / `JmapResponse` / `JmapBatch`, and `AddedItem` are now accessible at the `rfc8620::` root (e.g. `rfc8620::JmapSession`, `rfc8620::JmapBatch`). The dedicated `error.rs`, `filter.rs`, `redirect.rs`, `result_reference.rs`, `session.rs` files are gone.

- Restructured `rfc8620/event_source/` into a sub-module with `mod.rs`, private `types.rs` (re-exported), `coroutine.rs` (`JmapEventSourceYield`), `subscribe.rs` (`JmapEventSource` + `JmapEventSourceError`), and `utils.rs` (`parse_state_change`, `subscribe_url`). Public paths move from `rfc8620::event_source::JmapEventSource` to `rfc8620::event_source::subscribe::JmapEventSource`; type re-exports keep `rfc8620::event_source::StateChange` working.

[unreleased]: https://github.com/pimalaya/io-jmap/compare/root..HEAD
