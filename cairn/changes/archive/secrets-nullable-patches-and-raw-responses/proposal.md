---
cairn: change
id: secrets-nullable-patches-and-raw-responses
status: landed
created: 2026-11-13
---

# Redacted secrets, patches that can say null, responses decoded once

## Why

Three findings of the family audit of 2026-11-13 (JMAP-19, JMAP-07, JMAP-08).

- The Authorization value is a `SecretString` everywhere, but the Web Push `auth` secret and the push `url`, a capability the server itself never returns (RFC 8620 §7.2.1), were plain strings under a derived `Debug`, and five trace logs printed the whole request URL, userinfo and query included, where a download or event source URL may carry a token.
- `JmapMailboxUpdate` typed `parent_id` and `role` as `Option<T>` skipped when `None`, so a patch could not carry the `null` RFC 8621 §2 gives both: a mailbox could not move to the top level nor lose its role, while the field doc said `None` did the first. Same shape for `reply_to` and `bcc` of an identity and `description` and `share_with` of an address book.
- Every response was parsed twice: the body into a `serde_json::Value` tree, then the tree into the typed response. The tree costs a map and one allocated string per property name per object, and makes the 256 MiB body cap useless as a memory bound, a body of `[0,0,0,...]` growing sixteen-fold.

## What

- `JmapPushSubscriptionKeys::auth` is a `SecretString` (written on the wire through its own serializer), `JmapPushSubscriptionCreate` gets a hand-written `Debug` redacting `url`, and the five traces print scheme, host and path.
- `Option<Option<T>>` for the six nullable properties, as `JmapVacationResponseUpdate` already does, the docs corrected.
- `JmapResponse::method_responses` keeps each result as a `Box<RawValue>` (serde_json's `raw_value` feature, the one dependency change, a feature of an existing dependency), and the coroutines decode it with one `from_str` into their typed response.

## Not in this change

- io-http 0.6 traces the whole request under its own target, URL included: what io-jmap logs is fixed, what its dependency logs is io-http's to fix.
- The request side of JMAP-08: method arguments still go through `to_value` then `to_vec`.
- A settable `max_body_len` on `JmapSend`: the wrappers hold their `JmapSend` privately, so it needs a place in their options.
- Blob streaming (JMAP-18).
