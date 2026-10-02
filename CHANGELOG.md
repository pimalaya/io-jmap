# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Fixed `JmapEventSource::subscribe_url` appending `types`, `closeafter` and `ping` to an `eventSourceUrl` that already names them as an RFC 8620 §7.3 URI template, which is the shape Fastmail and Stalwart send.

  The request carried the literal `types={types}` first: Stalwart refused it (HTTP 400) and Fastmail held a stream that never pushed. The three variables are now expanded in place, and only a URL naming none of them gets them appended.

## [0.4.1] - 2026-10-01

### Added

- Added `JmapEmailSubmissionSetArgs`, carrying the RFC 8621 §7.5 `onSuccessUpdateEmail` and `onSuccessDestroyEmail` arguments of `EmailSubmission/set`, so a sent email can leave the drafts mailbox in the same request. `JmapEmailSubmissionSet::new` and `JmapClientStd::email_submission_set` take it, or the create map alone as before.

## [0.4.0] - 2026-09-29

### Added

- Added `JmapClientStdConnectOptions`, whose `proxy` field tunnels the connection through a SOCKS5 or HTTP proxy.

### Changed

- **BREAKING**: `JmapClientStd::connect` takes `(url, http_auth, opts)`, the TLS configuration moving into `JmapClientStdConnectOptions`.

## [0.3.0] - 2026-08-15

### Added

- Added I/O-free JMAP for Calendars coroutines following draft-ietf-jmap-calendars-27.

  `Calendar/get`, `Calendar/changes`, `CalendarEvent/get` (with the `recurrenceOverridesBefore`, `recurrenceOverridesAfter`, `reduceParticipants` and `timeZone` extra arguments), `CalendarEvent/query` (batched with `CalendarEvent/get` via Result Reference, with server-side recurrence expansion) and `CalendarEvent/changes`. The CalendarEvent's JSCalendar payload (RFC 8984) is kept as raw JSON next to the typed `id`, `baseEventId`, `calendarIds`, `isDraft`, `isOrigin`, `utcStart` and `utcEnd` properties. Writing is left out until something writes.

  The module is `calendars` rather than `rfcNNNN`, the domain being the only one of the JMAP suite still in the working group; it is renamed the day the number exists, the type names being already stable. `JmapClientStd` gained the matching `calendar_get`, `calendar_changes`, `calendar_event_get`, `calendar_event_query` and `calendar_event_changes` methods.

### Changed

- Bumped pimalaya-stream to 0.3, which drops the `sasl` module it no longer owns and whose `Read` and `Write` retry a stream reporting it is not ready. **Behaviour change.**

  The `Tls` type this crate takes comes from that version, so a consumer must move with it. A blocking socket is not supposed to report `EAGAIN`, yet callers saw one surface mid-exchange and end the exchange with a bare `Resource temporarily unavailable (os error 35)`, macOS especially. The transport now retries such a failure for a minute before giving up with a `TimedOut` naming the budget, and arms a socket read deadline at connect time so a server going silent on a healthy connection stops blocking the caller forever.

- Bumped io-http to 0.5.
- Raised the minimum supported Rust version from 1.87 to 1.88, following pimalaya-stream and io-http.

### Fixed

- Fixed the generic method errors displaying the RFC's `Foo` placeholder as though it were a type name, which reached users as "JMAP AddressBook/changes failed: JMAP Foo/changes failed: …". `JmapGetError`, `JmapSetError`, `JmapQueryError`, `JmapChangesError` and `JmapQueryChangesError` now display only their own cause, leaving the concrete method name to the caller that knows it. The doc comments keep the `Foo/get` spelling, which is how RFC 8620 itself writes a generic method.

## [0.2.1] - 2026-07-25

### Added

- Added an optional `schemars` feature deriving `schemars::JsonSchema` on the RFC 8621 output types, so downstream tools can generate JSON Schemas describing JMAP command output.

  The feature is off by default and stays `no_std`: it pulls only schemars' `derive` (not `std`). It covers the Email object (with its addresses, headers, body parts and body values), Mailbox (with its rights), Thread, Identity, EmailSubmission (with its envelope and delivery status) and VacationResponse objects. `JmapMailboxRole`, which serializes as a plain string through a hand-written impl, is described as a string on the field rather than derived, so the schema matches the wire format.

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

[unreleased]: https://github.com/pimalaya/io-jmap/compare/v0.4.1..HEAD
[0.4.1]: https://github.com/pimalaya/io-jmap/compare/v0.4.0..v0.4.1
[0.4.0]: https://github.com/pimalaya/io-jmap/compare/v0.3.0..v0.4.0
[0.3.0]: https://github.com/pimalaya/io-jmap/compare/v0.2.1..v0.3.0
[0.2.1]: https://github.com/pimalaya/io-jmap/compare/v0.2.0..v0.2.1
[0.2.0]: https://github.com/pimalaya/io-jmap/compare/v0.1.0..v0.2.0
[0.1.0]: https://github.com/pimalaya/io-jmap/compare/root..v0.1.0
