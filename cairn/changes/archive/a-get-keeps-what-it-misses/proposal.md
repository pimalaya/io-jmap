---
cairn: change
id: a-get-keeps-what-it-misses
status: landed
created: 2026-11-14
---

# A get keeps what it misses

## Why

Himalaya is making each listing and each write report the items it could not serve, next to the ones it did, as JMAP does (pimalaya/himalaya, cairn/changes/a-result-carries-its-errors). io-jmap already returns most of what JMAP answers per item (`not_found` of `JmapEmailGet` and `JmapMailboxGet`, `not_updated`, `not_created` and `not_destroyed` of the set methods). Four answers are still lost or untyped:

- **The chained queries drop `notFound`.** `JmapEmailQuery`, `JmapMailboxQuery`, `JmapEmailSubmissionQuery`, `JmapCalendarEventQuery` and `JmapContactCardQuery` parse the chained `/get`'s `list` alone. An id the query named but the get did not find is dropped silently.
- **One bad object fails the whole call.** In `JmapEmailGet`, `JmapEmailQuery`, `JmapMailboxGet` and `JmapMailboxQuery`, an object of `list` that does not deserialize fails everything with `ParseGetResponse`, losing every other object.
- **Four standard SetError types are untyped.** `forbidden`, `overQuota`, `tooLarge` and `rateLimit` (RFC 8620 §5.3) read as `Other` in `JmapEmailSetItemError`, `JmapEmailCopyItemError`, `JmapEmailImportItemError` and `JmapMailboxSetItemError`, whereas `JmapEmailSubmissionSetItemError` models them. A caller mapping a reason to a code has to match the type's name as a string.
- **Recipient details are dropped.** `JmapEmailSubmissionSetItemError::InvalidRecipients` drops the `invalidRecipients` list a server SHOULD send (RFC 8621 §7.5), and `TooManyRecipients` drops `maxRecipients`. A caller cannot say which addresses were refused.

## What

- Every chained query output gains `not_found: Vec<String>`, the chained `/get`'s `notFound`, in the order the server sent it.
- `JmapEmailGet`, `JmapEmailQuery`, `JmapMailboxGet` and `JmapMailboxQuery` read each object of `list` on its own. An object that does not deserialize is returned in a new `unparsed: Vec<JmapUnparsed>` field instead of failing the call.
  - `JmapUnparsed` is `{id, error}`: `id` the object's `id` when it is a string, `error` what the deserialization said.
  - `ParseGetResponse` stays for a response whose `list` is no array.
- `JmapEmailSetItemError`, `JmapEmailCopyItemError`, `JmapEmailImportItemError` and `JmapMailboxSetItemError` gain the variants `Forbidden`, `OverQuota`, `TooLarge` and `RateLimit`, each with its `description`, displayed `JMAP <type>[: <description>]` like the others.
- `JmapEmailSubmissionSetItemError::InvalidRecipients` gains `invalid_recipients: Vec<String>`, and `TooManyRecipients` gains `max_recipients: Option<u64>`. Both read `[]` and `None` when absent.

## Scope / non-goals

- The other data types (calendars, contacts, identities, vacation) keep failing a get whole on an object that does not deserialize. They can adopt `unparsed` when a caller needs it.
- `JmapIdentitySetItemError` keeps its variants.

## Decisions

- **Breaking.** A new public field on the outputs, and new variants on enums that are not `#[non_exhaustive]`, break exhaustive matches and struct literals. This is a 0.x minor release, and the owner decides the version.
