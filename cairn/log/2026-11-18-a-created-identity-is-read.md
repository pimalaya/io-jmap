---
cairn: log
change: a-created-identity-is-read
landed: 2026-11-18
---

# A created identity is read

Landed without a proposal: a bug found live on Fastmail by himalaya's session on 2026-11-18, `himalaya jmap identity create` reporting "created without an id" and exiting 1 although the identity was created, so a retry duplicated it.

`JmapIdentitySet` parsed `created` and `updated` as raw JSON and answered both empty. A `created` entry holds the id and the properties the server set or the client left out (RFC 8620 §5.3), Fastmail's `{"id": ..., "mayDelete": true}`, so it does not read as a `JmapIdentity` alone: the coroutine now keeps the create objects it sent and lays each entry over the one it answers. An `updated` entry, `null` or what the server changed beyond the patch, reads as an identity when it holds a whole one with its id, `None` otherwise, the key kept either way. Test `a_created_identity_reads_from_its_create_and_the_answer`, which fails on edc080f (`no entry found for key`).

The other `Foo/set` coroutines go through the generic `JmapSet<T>`, which reads each entry as a `T`: every such type reads one holding its id alone (`every_set_type_reads_a_created_entry_holding_its_id_alone`, src/rfc8620/set.rs), so none drops what it created.

Also fixed: the `ical` feature did not enable ical-rs's `parser`, which `JmapCalendarEvent::to_ical` (edc080f) needs for `Display`; tests passed through the dev-dependency, a plain `cargo build --all-features` did not.

Capabilities moved: none (responses already required every created object read).
