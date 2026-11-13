---
cairn: log
change: set-errors-kept
landed: 2026-11-12
---

# Set errors keep the type the server named

A Himalaya live test on Fastmail (2026-11-12) printed `error: unknown` for a `Mailbox/set` create the server refused with `alreadyExists`, and a second `message add` of one message failed without saying why: the catch-all `#[serde(other)] Unknown` of every per-object SetError enum dropped the type and the description, and `JmapEmailImportItemError` had no `alreadyExists`.

The catch-all of the ten SetError enums is now an untagged `Other { type_name, description }` variant, tried after the tagged ones, so an unmodelled type (or a modelled one whose fields do not fit) keeps what the server sent and displays as `JMAP <type>[: <description>]`; the `Foo/set` parsers fall back to `Other` with an empty `type_name` for a value that is no SetError object. `JmapEmailImportItemError` gained `AlreadyExists { existing_id, description }` (RFC 8621 §4.8), and the `alreadyExists` of `JmapEmailCopyItemError` and `JmapContactCardCopyItemError` now carries `existing_id` too (RFC 8620 §5.4). **Breaking**: `Unknown` is gone from these enums and `AlreadyExists` gained a field. Section references of `Email/import` (§4.8) and `Email/copy` (§4.7) were corrected on the way.

Capabilities moved: responses (new: "Set errors keep what the server named").

Verified: `a_duplicate_names_the_existing_email` and `an_unmodelled_error_keeps_its_type_and_description` (src/rfc8621/email/import.rs) and the `somethingNew` case of `calendar_event_set_types_each_refusal` (tests/calendars.rs) fail to compile against 0.7.0, which has neither variant, and pass now.
