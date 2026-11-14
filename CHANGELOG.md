# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added `JmapUnparsed`, an object of a `Foo/get` `list` that did not deserialize, with its `id` when it is a string and the error message.
- Added `Display` to `JmapEmailCopyItemError` and `JmapMailboxSetItemError`, as `JMAP <type>[: <description>]`.
- Added `Display` to `JmapIdentitySetItemError`, as `JMAP <type>[: <description>]`.
- Added `Display` to `JmapAddressBookSetItemError`, as `JMAP <type>[: <description>]`.
- Added `Display` to `JmapContactCardCopyItemError`, as `JMAP <type>[: <description>]`.

### Changed

- **BREAKING**: Added `not_found` to `JmapEmailQueryOutput`, `JmapMailboxQueryOutput`, `JmapEmailSubmissionQueryOutput`, `JmapCalendarEventQueryOutput` and `JmapContactCardQueryOutput`.

  The chained `/get`'s `notFound`, which was dropped.

- **BREAKING**: Added `unparsed` to `JmapEmailGetOutput`, `JmapEmailQueryOutput`, `JmapMailboxGetOutput` and `JmapMailboxQueryOutput`.

  An object of the `list` that does not deserialize is returned there instead of failing the whole call.

- **BREAKING**: Added `Forbidden`, `OverQuota`, `TooLarge` and `RateLimit` to `JmapEmailSetItemError`, `JmapEmailCopyItemError`, `JmapEmailImportItemError` and `JmapMailboxSetItemError`.

  These standard types (RFC 8620 §5.3) read as `Other` before.

- **BREAKING**: Added `invalid_recipients` to `JmapEmailSubmissionSetItemError::InvalidRecipients` and `max_recipients` to `TooManyRecipients` (RFC 8621 §7.5).

## [0.9.0] - 2026-11-13

### Changed

- **BREAKING**: Changed the pimalaya-stream dependency to 0.4.

  Its `tls::Tls` moved the trusted certificate from `cert` to `server`, and gained `client`, the certificate presented when the server asks for one (mutual TLS).

- **BREAKING**: Changed the io-http dependency to 0.8.

## [0.8.0] - 2026-11-13

### Added

- Added `AlreadyExists { existing_id, description }` to `JmapEmailImportItemError` (RFC 8621 §4.8): a server forbidding duplicates names the email it already holds.
- Added `max_frame_len` to `JmapEventSource`, 64 KiB by default (`JMAP_DEFAULT_MAX_FRAME_LEN`).
- Added `max_chunk_len` to `JmapEventSource`, 1 MiB by default (`JMAP_DEFAULT_MAX_CHUNK_LEN`).
- Added `JmapRequestError`, the request-level errors of RFC 8620 §3.6.1 (`unknownCapability`, `notJSON`, `notRequest`, `limit`), read from the problem details of a refused request.
- Added `JmapVacationResponseSetItemError`, the per-object error of `VacationResponse/set`.
- Added `email_set` to `JmapEmailSubmissionSetOutput`: the answer to the `Email/set` the server runs for `onSuccessUpdateEmail` and `onSuccessDestroyEmail` (RFC 8621 §7.5), which was dropped.
- Added `JmapEventSourceOptions`, whose `last_event_id` is sent as `Last-Event-ID` so a server pushes what changed since a previous connection (RFC 8620 §7.3).
- Added `JmapSession::account_for`, the primary account of a capability or a `JmapSessionAccountError` when the session names none.

### Changed

- **BREAKING**: Changed the io-http dependency to 0.7.

  The coroutines take and yield its types, so a consumer bumps in step for a single version to resolve. Its send and read errors, carried by the `Send`, `ReadHeaders` and `ReadChunks` variants, gained variants.

- **BREAKING**: Changed `JmapEventSourceError` to fail with `InvalidRequest`.

  `JmapEventSource::new` refuses a header value holding a CR, LF or NUL, the `Authorization` one included.

- **BREAKING**: `JmapResponse::method_responses` keeps each result as a `Box<RawValue>`, the JSON text the server sent, where it held a `serde_json::Value`.

  Every coroutine decodes it once into its typed response, so properties the library does not model cost no allocation. Enables the `raw_value` feature of serde_json.

- **BREAKING**: `JmapMailboxUpdate` takes `Option<Option<T>>` for `parent_id` and `role`, `JmapIdentityUpdate` for `reply_to` and `bcc`, `JmapAddressBookUpdate` for `description` and `share_with`: `Some(None)` clears the property (`null`), so a mailbox can move to the top level.
- **BREAKING**: `JmapPushSubscriptionKeys::auth` is a `SecretString`, and the `Debug` of `JmapPushSubscriptionCreate` leaves its `url` out.
- **BREAKING**: `JmapEventSource::new` takes a `JmapEventSourceOptions`, and `JmapEventSourceError` gained `InvalidLastEventId`.
- **BREAKING**: `JmapEventSourceYield::Frame` is now `Frame { change, id }`, carrying the last event id the server set.
- **BREAKING**: `JmapEventSource` skips a frame naming another event than `state`, which used to be read as a StateChange and to fail the stream when it was none; an empty `types` subscribes to `*` instead of sending `types=`.
- **BREAKING**: Every method coroutine built from a session fails with the new `Account` variant of its error when the session names no primary account for its capability, where it sent an empty `accountId`.
- `JmapClientStd::session_get` follows a redirect staying on the same origin and connection, five at most, which `/.well-known/jmap` discovery commonly gets; any other redirect still fails with `UnexpectedRedirect`.
- **BREAKING**: `JmapEventSourceError` gained `FrameTooLarge`: an SSE frame whose line or data exceeds `max_frame_len` fails the stream instead of growing without bound. A chunk over `max_chunk_len` fails with `ReadChunks(ChunkTooLarge)`, where the limit was 256 MiB.
- **BREAKING**: `JmapSendError` gained `Request { status, error }`: a non-2xx answer whose body names a problem of the JMAP namespace no longer reads as `HttpStatus`.
- **BREAKING**: `JmapMethodError` lost `UnknownCapability`, `NotJson`, `NotRequest` and `Limit`, which are request-level errors and now live in `JmapRequestError`.
- **BREAKING**: `JmapVacationResponseSetError` gained `NotUpdated`: an update the server refuses under `notUpdated` fails instead of completing as a success.
- **BREAKING**: `JmapBlobUpload::new` returns a `Result`, and `JmapBlobUploadError` gained `InvalidContentType`: a content type holding a CR, LF or NUL is refused instead of written into the request head.
- **BREAKING**: The catch-all `Unknown` of every per-object SetError enum (`JmapMailboxSetItemError`, `JmapEmailSetItemError`, `JmapEmailImportItemError`, `JmapEmailCopyItemError`, `JmapIdentitySetItemError`, `JmapEmailSubmissionSetItemError`, `JmapAddressBookSetItemError`, `JmapContactCardSetItemError`, `JmapContactCardCopyItemError`, `JmapCalendarEventSetItemError`) is now `Other { type_name, description }`, keeping the type and description the server sent.
- **BREAKING**: `AlreadyExists` of `JmapEmailCopyItemError` and `JmapContactCardCopyItemError` carries the `existing_id` RFC 8620 §5.4 requires; one without it reads as `Other`.
- **BREAKING**: `JmapMethodError::Unknown` is now `Other { type_name, description }`, keeping the type and description of a method error this library does not model (`fromAccountNotFound`), and `InvalidArguments` carries the `arguments` some servers (Fastmail) name, displayed as `JMAP invalidArguments [filter/inMailbox]`. A method response named `error` whose arguments are no error object reads as `Other` with an empty `type_name` and the parse failure as description.
- **BREAKING**: `JmapVacationResponseUpdate` takes `Option<Option<String>>` for `from_date`, `to_date`, `subject`, `text_body` and `html_body`: `None` leaves the property unchanged, `Some(None)` clears it (`null`), which the previous `Option<String>` could not say.

### Fixed

- The chained `Foo/query` + `Foo/get` coroutines (`JmapMailboxQuery`, `JmapEmailQuery`, `JmapEmailSubmissionQuery`, `JmapCalendarEventQuery`, `JmapContactCardQuery`) return the objects in the order of the query ids: RFC 8620 §5.1 leaves the order of a `/get` list to the server, and Fastmail's `Mailbox/get` ignores the sort.
- `VacationResponse/set` reads a `newState: null` (Fastmail) as an empty `new_state` rather than failing after the update was applied.
- `VacationResponse/set` reads the partial object Fastmail echoes under `updated` (the properties it was sent) instead of failing for want of an `id`; `updated` is filled only when the echo is a whole VacationResponse.
- `JmapEmailPatch` escapes the keyword and the mailbox id of its patch paths as JSON Pointer tokens (RFC 6901 §3): the keyword `todo/later` is patched as `keywords/todo~1later`.
- `EmailSubmission/set`, its cancel flow, `Email/import`, `Email/copy` and `ContactCard/copy` read a `newState` the server leaves out or answers `null` as an empty `new_state`, instead of failing and hiding the per-object errors.
- `Email/parse` reads a `parsed: null` (RFC 8621 §4.9) as an empty map instead of failing and hiding `notParsable` and `notFound`.
- The `Host` header keeps a port that is not the scheme default: the coroutines no longer set it by hand and leave it to io-http.
- `Email/set` sends the emails to create without the properties left unset, which were written as `null`, server-set ones included (RFC 8620 §5.3).
- The trace logs name a request URL by its scheme, host and path, without the userinfo and the query, either of which may hold a credential.

## [0.7.0] - 2026-11-03

### Added

- Added `anchor`, `anchorOffset` (RFC 8620 §5.5), `calculateTotal` and `collapseThreads` (RFC 8621 §4.4) to `JmapEmailQueryOptions`. `calculate_total` defaults to `true`, as it was always sent.
- Added `JmapEmailComparator::sent_at_desc`.
- Added `JmapMailCapability`, the RFC 8621 §1.3.1 account limits (`maxMailboxesPerEmail`, `maxMailboxDepth`, `emailQuerySortOptions`…), read by `JmapSession::mail_capability`.
- Added the `vcard` feature, converting a ContactCard to and from vCard (RFC 9555): `JmapContactCard::to_vcard`, `from_vcard`, and `update_from_vcard` answering the `ContactCard/set` patch against a base vCard.
- Added the `ical` feature, converting a parsed iCalendar object to a CalendarEvent, its one JSCalendar 2.0 entry: `JmapCalendarEvent::from_ical`.
- Added `JmapCalendarEventPatch::diff`, the `CalendarEvent/set` patch between two events, `recurrenceOverrides` patched per recurrence id.
- Added `max_len` to `JmapBlobDownload` and `max_blob_len` to `JmapClientStd`, 256 MiB by default.

### Changed

- **BREAKING**: `JmapMethodError` gained `AnchorNotFound`, `UnsupportedSort` and `UnsupportedFilter` (RFC 8620 §5.5), which used to read as `Unknown`. `JmapEmailQueryOptions` literals without `..Default::default()` need the new fields.
- **BREAKING**: Bumped io-http to 0.6. The coroutines take and yield its types, so a consumer bumps in step for a single version to resolve.

  Its `Http11SendError`, carried by every `Send` variant, gained `Eof`, `HeadersTooLarge` and `BodyTooLarge`: a response head over 64 KiB or a body over 256 MiB now fails the exchange.

- Changed the minimum Rust version to 1.89.

## [0.6.0] - 2026-10-09

### Added

- Added the `CalendarEvent/set` coroutine (draft-ietf-jmap-calendars-27), `JmapCalendarEventSet`: create, update by patch, destroy, `ifInState` and `sendSchedulingMessages`, its per-object errors typed as `JmapCalendarEventSetItemError` (the RFC 8620 §5.3 ones and the draft's `noSupportedScheduleMethods`) with `Display`. `JmapClientStd` gained the matching `calendar_event_set` method.

### Changed

- **BREAKING**: `JmapMethodError` gained `StateMismatch` (RFC 8620 §5.3), which used to read as `Unknown`, so a `Foo/set` refused for its `ifInState` can be told apart. `JmapClientStdError` gained `CalendarEventSet`.

### Fixed

- Fixed `JmapCalendarEvent` serializing the server-set `isOrigin`, which a create must not carry (RFC 8620 §5.3).
- Fixed every `Foo/set` failing to parse a response naming no `newState`, which Stalwart answers a call that changed nothing; it reads as empty.

## [0.5.0] - 2026-10-09

### Added

- Added `JmapCoreCapability`, the RFC 8620 §2 server limits (`maxSizeUpload`, `maxObjectsInGet`, `maxObjectsInSet`…), read by `JmapSession::core_capability`.
- Added `JmapSession::resolve_upload_url` and `JmapSession::resolve_download_url`, filling the RFC 8620 §6 templates with their variables percent-encoded.
- Added `Display` to `JmapEmailSetItemError`, `JmapEmailImportItemError`, `JmapEmailSubmissionSetItemError` and `JmapContactCardSetItemError`, reading as the wire type and the server's description.

### Changed

- **BREAKING**: `JmapEmailSubmissionSetItemError` gained `TooLarge` (RFC 8621 §7.5, with its `maxSize`), `Forbidden`, `OverQuota` and `RateLimit` (RFC 8620 §5.3), which used to read as `Unknown`, so a refusal for good can be told from one to retry.

### Fixed

- Fixed `JmapEmailSubmissionSetItemError::CannotUnsendMessage` missing the RFC 8621 §7.5 spelling `cannotUnsend`.
- Fixed `Email/import`, `Email/copy` and `ContactCard/copy` failing to parse a `created` or `notCreated` the server answers as `null`, which RFC 8620 §5.3 allows and Fastmail does whenever nothing was refused.

## [0.4.2] - 2026-10-02

### Fixed

- Fixed `JmapEventSource::subscribe_url` ignoring the RFC 8620 §7.3 `eventSourceUrl` template.

  The `{types}`, `{closeafter}` and `{ping}` variables are now expanded in place.

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

[unreleased]: https://github.com/pimalaya/io-jmap/compare/v0.9.0..HEAD
[0.9.0]: https://github.com/pimalaya/io-jmap/compare/v0.8.0..v0.9.0
[0.8.0]: https://github.com/pimalaya/io-jmap/compare/v0.7.0..v0.8.0
[0.7.0]: https://github.com/pimalaya/io-jmap/compare/v0.6.0..v0.7.0
[0.6.0]: https://github.com/pimalaya/io-jmap/compare/v0.5.0..v0.6.0
[0.5.0]: https://github.com/pimalaya/io-jmap/compare/v0.4.2..v0.5.0
[0.4.2]: https://github.com/pimalaya/io-jmap/compare/v0.4.1..v0.4.2
[0.4.1]: https://github.com/pimalaya/io-jmap/compare/v0.4.0..v0.4.1
[0.4.0]: https://github.com/pimalaya/io-jmap/compare/v0.3.0..v0.4.0
[0.3.0]: https://github.com/pimalaya/io-jmap/compare/v0.2.1..v0.3.0
[0.2.1]: https://github.com/pimalaya/io-jmap/compare/v0.2.0..v0.2.1
[0.2.0]: https://github.com/pimalaya/io-jmap/compare/v0.1.0..v0.2.0
[0.1.0]: https://github.com/pimalaya/io-jmap/compare/root..v0.1.0
