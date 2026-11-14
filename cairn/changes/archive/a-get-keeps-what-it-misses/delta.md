---
cairn: delta
change: a-get-keeps-what-it-misses
---

# Delta

## ADDED Requirements

### Requirement: An object that does not deserialize is reported, not fatal
(responses) `JmapEmailGet`, `JmapEmailQuery`, `JmapMailboxGet` and `JmapMailboxQuery` SHALL read each object of the `/get` `list` on its own. An object that does not deserialize SHALL be returned in `unparsed` as a `JmapUnparsed { id, error }`, `id` the object's `id` when it is a string and `error` the deserialization failure, the other objects returned as usual. `ParseGetResponse` SHALL stay for a response whose `list` is no array.

#### Scenario: One malformed email among two
- GIVEN an `Email/get` answering `list: [{"id": "e1", …}, {"id": "e2", "receivedAt": 42}]`
- WHEN `JmapEmailGet` completes
- THEN its `emails` hold `e1` and its `unparsed` holds `{id: Some("e2"), error: …}`

## MODIFIED Requirements

### Requirement: A chained query keeps the query order
(responses) Every coroutine chaining a `Foo/query` into a back-referenced `Foo/get` (`JmapMailboxQuery`, `JmapEmailQuery`, `JmapEmailSubmissionQuery`, `JmapCalendarEventQuery`, `JmapContactCardQuery`) SHALL return the fetched objects in the order of the query `ids`, an object the query did not name last, whatever order the server lists them in, and SHALL return the `/get`'s `notFound` as `not_found`, in the order the server sent it.

#### Scenario: A server listing in its own order
- GIVEN `Mailbox/query` answering `ids: ["m3", "m1", "m2"]` and `Mailbox/get` listing `m1`, `m2`, `m3`
- WHEN `JmapMailboxQuery` completes
- THEN its `mailboxes` are `m3`, `m1`, `m2`

#### Scenario: An email gone between the query and the get
- GIVEN `Email/query` answering `ids: ["e1", "e2"]` and `Email/get` listing `e1` with `notFound: ["e2"]`
- WHEN `JmapEmailQuery` completes
- THEN its `emails` hold `e1` and its `not_found` is `["e2"]`

### Requirement: Set errors keep what the server named
(responses) Every per-object `SetError` enum (`JmapMailboxSetItemError`, `JmapEmailSetItemError`, `JmapEmailImportItemError`, `JmapEmailCopyItemError`, `JmapIdentitySetItemError`, `JmapEmailSubmissionSetItemError`, `JmapAddressBookSetItemError`, `JmapContactCardSetItemError`, `JmapContactCardCopyItemError`, `JmapCalendarEventSetItemError`) SHALL read a type it does not model, or a modelled type whose other properties do not fit, as `Other { type_name, description }`, carrying the `type` and `description` the server sent, and SHALL display it as `JMAP <type>[: <description>]`. A `Foo/set` error that is no SetError object at all SHALL read as `Other` with an empty `type_name`.

`JmapEmailSetItemError`, `JmapEmailCopyItemError`, `JmapEmailImportItemError` and `JmapMailboxSetItemError` SHALL model the standard types `forbidden`, `overQuota`, `tooLarge` and `rateLimit` (RFC 8620 §5.3), each with its `description`. `JmapEmailSubmissionSetItemError` SHALL carry the `invalidRecipients` list of `invalidRecipients` as `invalid_recipients`, `[]` when absent, and the `maxRecipients` of `tooManyRecipients` as `max_recipients`, `None` when absent (RFC 8621 §7.5).

`alreadyExists` SHALL carry the `existingId` the server MUST name (RFC 8620 §5.4) as `existing_id`, on `Email/import` (RFC 8621 §4.8), `Email/copy` and `ContactCard/copy`; one naming no `existingId` reads as `Other`.

#### Scenario: A duplicate import
- GIVEN a server forbidding duplicate emails
- WHEN `Email/import` answers `notCreated: {"m0": {"type": "alreadyExists", "existingId": "E1"}}`
- THEN `not_created["m0"]` is `AlreadyExists { existing_id: "E1", .. }`, displayed `JMAP alreadyExists (existing id E1)`

#### Scenario: A refused keyword update
- WHEN `Email/set` answers `notUpdated: {"e1": {"type": "forbidden", "description": "read-only mailbox"}}`
- THEN `not_updated["e1"]` is `Forbidden { description: Some("read-only mailbox") }`, displayed `JMAP forbidden: read-only mailbox`

#### Scenario: Two refused recipients
- WHEN `EmailSubmission/set` answers `notCreated: {"s0": {"type": "invalidRecipients", "invalidRecipients": ["a@x", "b@y"]}}`
- THEN `not_created["s0"]` is `InvalidRecipients { invalid_recipients: ["a@x", "b@y"], .. }`

## REMOVED Requirements

None.
