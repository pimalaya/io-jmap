---
cairn: delta
change: secrets-nullable-patches-and-raw-responses
---

## ADDED Requirements

### Requirement: Method responses are decoded once
`JmapResponse` SHALL keep the result of each method response as the JSON text the server sent (`Box<RawValue>`), and every coroutine SHALL decode that text once, straight into its typed response: no generic JSON tree is built of a response, so a property the library does not model costs no allocation, and what a response holds on the heap follows its wire size, not the number of values in it. The per-object errors of a `Foo/set` stay generic values until their wrapper types them, being few and small.

#### Scenario: Emails carrying properties the library does not model
- GIVEN an `Email/get` response of 1000 emails, each an id and 20 unmodelled properties
- WHEN `JmapEmailGet` decodes it
- THEN it allocates fewer than 2000 times, one string per id and the buffers, where a JSON tree took over 46000

### Requirement: A patch can clear a nullable property
An update struct SHALL type a property its RFC makes nullable as `Option<Option<T>>`: `None` leaves it out of the patch, `Some(None)` writes `null`, `Some(Some(value))` writes the value. This holds for `parent_id` and `role` of `JmapMailboxUpdate` (RFC 8621 §2), `reply_to` and `bcc` of `JmapIdentityUpdate` (RFC 8621 §6), `description` and `share_with` of `JmapAddressBookUpdate` (RFC 9610 §2), and the five nullable properties of `JmapVacationResponseUpdate`.

#### Scenario: A mailbox moved to the top level
- GIVEN the patch `JmapMailboxUpdate { parent_id: Some(None), .. }`
- WHEN it is serialised
- THEN it reads `{"parentId": null}`

### Requirement: Secrets stay out of Debug and logs
`JmapPushSubscriptionKeys` SHALL hold its `auth` secret as a `SecretString`, written on the wire and redacted by `Debug`, and the `Debug` of `JmapPushSubscriptionCreate` SHALL show `[REDACTED]` for its `url`, a push URL being a capability the server itself never returns (RFC 8620 §7.2.1).

The library SHALL log a request URL as its scheme, host and path only, never its userinfo, query or fragment: a session, download or event source URL may carry an access token in its query, a configured one a password. This binds what io-jmap logs; io-http 0.6 traces the whole request, its URL included, under its own target.

#### Scenario: A URL carrying a token
- GIVEN the URL `https://user:hunter2@api.example.com/jmap/x?token=s3cr3t`
- WHEN the five request coroutines are built with trace logging on
- THEN each logs `https://api.example.com/jmap/x`, and no record of the library holds `hunter2` or `s3cr3t`

## MODIFIED Requirements

## REMOVED Requirements
