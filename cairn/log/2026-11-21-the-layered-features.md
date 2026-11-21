---
cairn: log
change: the-layered-features
landed: 2026-11-21
---

# The layered features

Landed without a proposal (the family's cargo-006 feature shape, the one io-imap, io-smtp and io-webdav ship, applied as a packaging change).

**Features.** The old `client` feature, which gated the blocking client and std, is renamed `std`. `client` now gates the coroutines (`coroutine`, `rfc8620::coroutine` and every method submodule of `rfc8620`, `rfc8621`, `rfc9610` and `calendars`) and makes io-http, log and secrecy optional; it is on by default and implied by `std`, which the TLS features imply. The vocabulary stays compiled under no feature: the capabilities, the session, request, error, filter and event source objects, and the data types.

**Gated beside the modules.** `JmapContactCard::update_from_vcard` and its `diff` helper (src/rfc9610/contact_card/vcard.rs) build a `JmapContactCardPatch`, which lives in the `set` coroutine module, so they need `client`; `to_vcard`, `from_vcard`, `from_ical` and `to_ical` do not. The client-only private helpers `JMAP_ERROR_URN`, `JmapRequestError::from_body`, `JmapMethodError::unreadable` (src/rfc8620/error.rs) and `JmapCloseAfter::as_str` (src/rfc8620/event_source.rs) carry their own `cfg`, as do the converter tests reading a patch.

**Verified.** Builds with no features, `client`, `std`, default, all features, and no features plus `ical`, `vcard`, `schemars` are warning-free; `cargo test --all-features` and the lib tests with `ical`, `vcard`, `schemars` alone pass; clippy over all features and all targets and `cargo fmt --check` are clean. CI runs `cargo test --all-features` and the live suites on default features, both valid.

Capabilities moved: none (no spec states the feature layering; calendars' `ical` requirement is unchanged).
