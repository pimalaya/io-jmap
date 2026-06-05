# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-06-05

### Added

- Added the `JmapCoroutine` mirroring `core::ops::Coroutine`.

  The trait is composed of `Yield` and `Return` associated types, as well as a two-variant `JmapCoroutineState<Y, R>` (`Yielded(Y)` and `Complete(R)`). Standard coroutines pick the shared `JmapYield { WantsRead, WantsWrite(Vec<u8>) }`; the three redirect-capable coroutines (`JmapSessionGet`, `JmapBlobUpload`, `JmapBlobDownload`) declare their own `JmapRedirectYield` with an extra `WantsRedirect { url, keep_alive, same_origin }` variant.

- Added the `jmap_try!` macro: coroutine equivalent of `?`.

  Advances one inner resume step, re-yields intermediate `Yielded(y)` (via `Into`), and short-circuits on `Complete(Err(_))`.

- Added I/O-free JMAP Core coroutines following RFC 8620.

  session-get (with `/.well-known/jmap` discovery), send (single `JmapRequest` over HTTP/1.1), get, set, query, changes, query-changes (generic over the JMAP method name and capabilities), blob-upload and blob-download.

- Added I/O-free JMAP for Mail coroutines following RFC 8621.

  `Mailbox/get`, `Mailbox/set`, `Mailbox/query` (batched with `Mailbox/get` via Result Reference), `Mailbox/changes`, `Email/get`, `Email/set`, `Email/query` (batched with `Email/get`), `Email/changes`, `Email/copy`, `Email/import`, `Email/parse`, `Thread/get`, `Thread/changes`, `Identity/get`, `Identity/set`, `EmailSubmission/get`, `EmailSubmission/set`, `EmailSubmission/query` (batched), `EmailSubmission/set` cancel, `VacationResponse/get`, `VacationResponse/set`.

- Added I/O-free JMAP Event Source streaming coroutine following RFC 8620 §7.2.

  Composes `Http11ReadHeaders` + `Http11ReadChunksStream` + `SseFrameParser` + `parse_state_change` into a single state machine. Yields one `JmapStateChange` per push frame; empty SSE frames surface as the default state change (keep-alive). Supports cooperative shutdown via a shared `AtomicBool`.

- Added the `client` cargo feature enabling `JmapClientStd::new(stream, http_auth)`.

  Blocking light client wrapping any `Read + Write` stream and exposing one method per JMAP coroutine. Caches the discovered `JmapSession` after the first `session_get` and resolves `accountId` and `apiUrl` from it on subsequent calls.

- Added the `rustls-ring` cargo feature (default) enabling `JmapClientStd::connect(url, tls, http_auth)`.

  Opens `http://` / `https://` (or `jmap://` / `jmaps://`) URLs via [pimalaya/stream](https://github.com/pimalaya/stream) with rustls + ring crypto provider, runs the TLS handshake when needed, and sets a 5 s per-read timeout so long-lived watch loops can poll their shutdown atomic between push frames.

- Added the `rustls-aws` cargo feature.

  Same full client as `rustls-ring` but with the aws-lc-rs crypto provider.

- Added the `native-tls` cargo feature.

  Same full client backed by the platform's `native-tls` implementation.

- Added the `vendored` cargo feature.

  Compiles the underlying TLS dependencies in vendored mode (forwarded to `pimalaya-stream/vendored`).

[unreleased]: https://github.com/pimalaya/io-jmap/compare/v0.1.0..HEAD
[0.1.0]: https://github.com/pimalaya/io-jmap/compare/root..v0.1.0
