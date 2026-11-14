---
cairn: log
change: a-get-keeps-what-it-misses
landed: 2026-11-14
---

# A get keeps what it misses

For Himalaya's per-item errors (pimalaya/himalaya, cairn/changes/a-result-carries-its-errors), io-jmap now returns what a get or a set could not serve instead of dropping it or failing whole.

The five chained queries (rfc8621/email/query.rs, rfc8621/mailbox/query.rs, rfc8621/email_submission/query.rs, calendars/calendar_event/query.rs, rfc9610/contact_card/query.rs) read the chained `/get`'s `notFound` into a new `not_found` field.

`JmapEmailGet`, `JmapEmailQuery`, `JmapMailboxGet` and `JmapMailboxQuery` read each object of `list` on its own through the `pub(crate)` `JmapGetItem<T>` (rfc8620/get.rs), which borrows the object's JSON text as a `&RawValue` and decodes it alone. An object that fails becomes a `JmapUnparsed { id, error }` in the new `unparsed` field, `id` read from the same text when it is a string. Borrowing keeps the "decoded once" requirement: the allocation test of 1000 emails still passes under its bound. The plain gets reach it through `JmapGet<JmapGetItem<T>>`, so `JmapGet` and the other data types are unchanged and still fail whole.

`JmapEmailSetItemError`, `JmapEmailCopyItemError`, `JmapEmailImportItemError` and `JmapMailboxSetItemError` gained `Forbidden`, `OverQuota`, `TooLarge` and `RateLimit`, each with its `description` only, as the proposal set. `JmapEmailSubmissionSetItemError::TooLarge` keeps its `max_size`. `JmapEmailCopyItemError` and `JmapMailboxSetItemError` had no `Display` although the spec said every SetError enum displays; they gained one in the shape of the others. `InvalidRecipients` gained `invalid_recipients`, displayed `JMAP invalidRecipients [a@x, b@y][: <description>]` like `invalidProperties`, and `TooManyRecipients` gained `max_recipients`, displayed ` (at most N recipients)` after the description like `tooLarge`'s size.

**Breaking**: the new output fields and variants, none of the types being `#[non_exhaustive]`.

Deviations from the proposal: `error` of `JmapUnparsed` is a `String`, the failure's message, since the outputs derive `Clone` and `serde_json::Error` is not `Clone`. The delta named `ParseGetResponse` for a `list` that is no array; that variant exists on the chained queries only, the plain gets failing with `Get(ParseResponse)`, and the spec names both.

Capabilities moved: responses ("Set errors keep what the server named" and "A chained query keeps the query order" modified; new: "An object that does not deserialize is reported, not fatal").

Left open: `JmapIdentitySetItemError`, `JmapAddressBookSetItemError` and `JmapContactCardCopyItemError` still have no `Display`, which the spec's set errors requirement claims for every enum.

Verified: the new tests do not build against the old code (new fields and variants), and the standard types read as `Other` there. `an_unmodelled_error_keeps_its_type_and_description` in rfc8621/email/import.rs used `overQuota` as its unmodelled type; it now uses `blobNotFound`. Tests: `email_query_keeps_what_its_get_did_not_find`, `mailbox_query_keeps_a_mailbox_that_does_not_deserialize`, `email_get_keeps_an_email_that_does_not_deserialize`, `email_get_list_that_is_no_array_fails`, `the_standard_set_errors_are_typed` and `email_set_reads_a_refused_keyword_update` in tests/rfc8621.rs; `invalid_recipients_keeps_the_addresses_the_server_lists`, `invalid_recipients_without_a_list_reads_empty` and `too_many_recipients_keeps_the_limit_the_server_sends` in rfc8621/email_submission.rs.
