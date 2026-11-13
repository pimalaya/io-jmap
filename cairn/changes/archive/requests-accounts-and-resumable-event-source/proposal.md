---
cairn: change
id: requests-accounts-and-resumable-event-source
status: landed
created: 2026-11-13
---

# Requests, accounts and a resumable event source

## Why

Five findings of the family audit of 2026-11-13 (JMAP-11, JMAP-13, JMAP-16, JMAP-17, JMAP-20).

- The event source dropped the id of every frame and `new` took no last event id, so the replay RFC 8620 §7.3 builds on `Last-Event-ID` was out of reach: a change landing between two subscriptions waited for the next one. Beside it, an empty `types` sent `types=` where §7.3 defines `*`, and any frame, whatever its event, was read as a StateChange and ended the stream when it was none.
- Five coroutines set `Host` by hand from `host_str()`, which has no port, overriding the correct value io-http writes when no `Host` is set: a server on `:8443` received `Host: mail.example.org`.
- 35 constructors read the primary account with `unwrap_or_default()`: a session naming none sent `"accountId": ""` and got a server-side error in place of a local one.
- `Email/set` serialised a `JmapEmail` to create with every unset property as `null`, `id`, `blobId`, `threadId` and `size` included, which RFC 8620 §5.3 wants omitted.
- `/.well-known/jmap` commonly answers with a redirect to the session resource on the same origin, and `JmapClientStd::session_get` stopped at it, so the bare-authority discovery the README advertises failed where it is most used.

## What

- `JmapEventSourceYield::Frame { change, id }`, `JmapEventSourceOptions { last_event_id }` sent as `Last-Event-ID` and checked for CR, LF and NUL, `types` empty mapped to `*`, frames of another event than `state` skipped.
- The five `.header("Host", host)` lines deleted.
- `JmapSession::account_for(capability) -> Result<&str, JmapSessionAccountError>`, used by the 35 constructors through an `Account` variant on each error enum.
- `JmapEmailSetArgs::create` serialised through a function dropping the `null` members. Chosen over `skip_serializing_if` on `JmapEmail`, which would change what every consumer printing an email as JSON outputs, and over a dedicated `JmapEmailCreate`, which is a larger and breaking change for the same bytes on the wire.
- `JmapClientStd::session_get` follows a redirect when `same_origin && keep_alive`, five hops at most. The coroutine is untouched: it yields the redirect, the policy is the client's.

## Not in this change

An optional account id per request, the second half of JMAP-16. The constructors have no common place for it (several take no options struct), so where it goes is a design decision. Until then a caller reaches another account by editing `primary_accounts` on its copy of the session.
