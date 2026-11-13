---
cairn: log
change: secrets-nullable-patches-and-raw-responses
landed: 2026-11-13
---

# Secrets redacted, patches that say null, responses decoded once

Three findings of the family audit of 2026-11-13, the last round of it for this crate.

JMAP-19: `JmapPushSubscriptionKeys::auth` (rfc8620/push_subscription/set.rs) became a `SecretString`, serialised through `serialize_auth` since a `SecretString` does not serialise by itself, and `JmapPushSubscriptionCreate` got a hand-written `Debug` showing `[REDACTED]` for `url`; `p256dh`, a public key, stays readable. The traces of `JmapSend`, `JmapSessionGet`, `JmapBlobUpload`, `JmapBlobDownload` and `JmapEventSource` print scheme, host and path. **Breaking**: the type of `auth`. What is left is outside the crate: io-http 0.6 logs `{req:?}` at trace level in `Http11Send::new`, and its `Debug` prints the whole URL, so a token in a query still reaches a trace log, under the `io_http` target.

JMAP-07: `parent_id` and `role` of `JmapMailboxUpdate`, `reply_to` and `bcc` of `JmapIdentityUpdate`, `description` and `share_with` of `JmapAddressBookUpdate` are `Option<Option<T>>`, `Some(None)` writing `null`. The field doc that promised a move to the top level on `None` is corrected. **Breaking**: the six field types; a caller writing `Some(x)` writes `Some(Some(x))`.

JMAP-08: `JmapResponse::method_responses` (rfc8620/request.rs) holds each result as a `Box<RawValue>` and the 20 coroutine files decode it with `serde_json::from_str` where they called `from_value` on a tree. serde_json gained its `raw_value` feature in Cargo.toml: a feature of an existing dependency, no new one, Cargo.lock unchanged. Measured by tests/alloc.rs with a counting allocator, per thread, alone in its binary: decoding an `Email/get` response of 1000 emails, each an id and 20 unmodelled properties, took 46029 allocations before and 1024 after, the same on every run. **Breaking**: the type of `method_responses`, for a caller reading raw responses (a passthrough command re-serialises a `RawValue` as it came, or parses it to a `Value` to pretty-print). Not done: the request side still builds a `Value` of the arguments before the bytes, `JmapSend` exposes no body limit of its own, and the types flattening a JSON payload (`JmapCalendarEvent`, `JmapContactCard`) still buffer each object once.

JMAP-18, blobs held whole, was left: it is a streaming API for upload and download, not a first step that fits beside these three.

Capabilities moved: responses (new: "Method responses are decoded once"), requests (new: "A patch can clear a nullable property", "Secrets stay out of Debug and logs").

Verified, red before the fix where the old API could express the test: `debug_redacts_the_url_and_the_auth_secret` failed on the URL in the `Debug` output, `logged_urls_carry_no_userinfo_nor_query` (tests/logs.rs) found `session url: https://user:hunter2@api.example.com/jmap/x?token=s3cr3t` and four like it, `unmodelled_properties_cost_no_allocation` (tests/alloc.rs) failed with `46029 allocations for 1000 emails`. The three nullable-patch tests (`mailbox_update_clears_a_nullable_property`, `identity_update_clears_a_nullable_property`, `update_clears_a_nullable_property`) do not compile against the old types, which is the defect: `Some(None)` could not be written.
