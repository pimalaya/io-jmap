# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-07-16

### Added

- Added I/O-free `PushSubscription/get` and `PushSubscription/set` coroutines following RFC 8620 §7.2.

  `JmapPushSubscriptionGet` and `JmapPushSubscriptionSet` build custom batches instead of reusing the generic `JmapGet`/`JmapSet`, as PushSubscription methods take no `accountId` or `ifInState` and return no state strings. The new `rfc8620::push_subscription` module also ships the `JmapPushSubscription` object, its create/update shapes, the Web Push encryption keys object and the `JmapPushVerification` payload the server POSTs to the subscription URL. `JmapClientStd` gained the matching `push_subscription_get` and `push_subscription_set` methods, and `JmapMethodError` a `Forbidden` variant.

- Added I/O-free JMAP for Contacts coroutines following RFC 9610.

  `AddressBook/get`, `AddressBook/changes`, `AddressBook/set` (with the `onDestroyRemoveContents` and `onSuccessSetIsDefault` extra arguments and the `addressBookHasContents` set error), `ContactCard/get`, `ContactCard/changes`, `ContactCard/query` (batched with `ContactCard/get` via Result Reference), `ContactCard/set`, `ContactCard/copy`. The ContactCard's JSContact payload (RFC 9553) is kept as raw JSON next to the typed `id` and `addressBookIds` properties.

### Changed

- Reorganised the type modules so each type lives next to the code that owns it, dropping the `types` catch-all modules and their flat re-exports; a type tied to a single method moved into that method's module and gained a path segment.

  In `rfc8621::email`, `JmapEmailFilter`, `JmapEmailComparator` and `JmapEmailSortProperty` moved to `email::query`, `JmapEmailPatch`/`JmapEmailPatchOp`/`JmapEmailSetItemError` to `email::set`, `JmapEmailImportArgs`/`JmapEmailImportItemError` to `email::import`, and `JmapEmailCopyArgs`/`JmapEmailCopyItemError` to `email::copy`; the create, update, filter, sort and per-object error companions of Mailbox, Identity, VacationResponse, EmailSubmission, AddressBook, ContactCard and PushSubscription moved into their own `set`, `query`, `copy` or `cancel` modules the same way. The shared RFC 8620 core types split by family: `JmapSession`/`JmapAccountInfo` into `rfc8620::session`, `JmapRequest`/`JmapResponse`/`JmapBatch`/`JmapResultReference` into `rfc8620::request`, `JmapMethodError` and the per-object `JmapSetError` into `rfc8620::error`, `JmapFilter`/`JmapFilterOperator`/`JmapFilterOperatorKind` into `rfc8620::filter`, and `JmapAddedItem` into `rfc8620::query_changes`. Entity objects, shared enums and constants keep their module-root path: `rfc8621::email::JmapEmail`, `JmapEmailAddress`, `JmapEmailProperty`, the `JMAP_KEYWORD_*` constants, `rfc9610::JmapContactsCapability` and every capability constant.

- Renamed the capability constants with the strict `Jmap` domain prefix.

  `rfc8620::CORE_CAPABILITY` became `JMAP_CORE_CAPABILITY`, `rfc8621::MAIL_CAPABILITY` became `JMAP_MAIL_CAPABILITY`, `rfc8621::email_submission::SUBMISSION_CAPABILITY` became `JMAP_SUBMISSION_CAPABILITY`, `rfc8621::vacation_response::VACATION_RESPONSE_CAPABILITY` became `JMAP_VACATION_RESPONSE_CAPABILITY` and `rfc9610::CONTACTS_CAPABILITY` became `JMAP_CONTACTS_CAPABILITY`.

- Renamed the standard email keyword constants and flattened them into the email module.

  The `rfc8621::email::keywords` module is gone; its `SEEN`, `FLAGGED`, `ANSWERED` and `DRAFT` constants are now `rfc8621::email::JMAP_KEYWORD_SEEN`, `JMAP_KEYWORD_FLAGGED`, `JMAP_KEYWORD_ANSWERED` and `JMAP_KEYWORD_DRAFT`.

- Moved the free function `rfc8620::event_source::parse_state_change` to the associated function `JmapStateChange::parse`.

- Moved the free function `rfc8620::event_source::subscribe_url` to the associated function `JmapEventSource::subscribe_url`.

- Moved the free function `client::default_alpn` to the associated function `JmapClientStd::default_alpn`.

- Renamed the `JmapClientStdError::JmapEmailCopyArgs` and `JmapClientStdError::JmapEmailImportArgs` variants to `EmailCopy` and `EmailImport`.

- Changed `JmapClientStdError::UrlUnsupportedScheme` from a tuple variant to a struct variant with `url` and `scheme` fields.

- Documented every public item, including struct fields and enum variants; docs.rs now builds with all features enabled.

- Reworked the library logging to the shared debug-plus-trace pattern and removed the per-resume state traces, along with the now-unused internal state Display implementations.

- Bumped io-http to 0.3 (adapting the event source coroutine to the renamed reader coroutines) and pimalaya-stream to 0.1, and removed the io-http git patch pinning an unpublished revision.

### Fixed

- Fixed the RFC 8620 section numbers cited by the Event Source docs: Event Source is §7.3 and StateChange is §7.1, not §7.2/§7.2.1 (which cover PushSubscription).

- Fixed the live provider tests against the io-http 0.2 auth renames (`HttpAuthBasic`, `HttpAuthBearer`).

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

[unreleased]: https://github.com/pimalaya/io-jmap/compare/v0.2.0..HEAD
[0.2.0]: https://github.com/pimalaya/io-jmap/compare/v0.1.0..v0.2.0
[0.1.0]: https://github.com/pimalaya/io-jmap/compare/root..v0.1.0
