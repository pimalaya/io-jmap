---
cairn: log
change: requests-accounts-and-resumable-event-source
landed: 2026-11-13
---

# Requests name their host and account, the event source can resume

Five findings of the family audit of 2026-11-13.

JMAP-11: `JmapEventSource` (rfc8620/event_source/subscribe.rs) yields `Frame { change, id }`, the id being the one io-http's SSE parser keeps across frames, and takes a `JmapEventSourceOptions` whose `last_event_id` goes out as `Last-Event-ID`, refused with `InvalidLastEventId` when it holds a CR, LF or NUL. An empty `types` subscribes to `*`. A frame naming another event than `state` is skipped: until now it was read as a StateChange, the `ping` event passing only because every property of a StateChange has a default, and any other failing the stream. A caller that counted the empty StateChange a ping surfaced as now sees its keep-alive in the bytes it reads, not in a yield. **Breaking**: the `Frame` variant, the signature of `new`, the new error variant, the skipped frames and the `*`. Not done here: a response framed by connection close is still refused as `NotChunked`.

JMAP-13: the `Host` header was set by hand in `JmapSend`, `JmapSessionGet`, `JmapBlobUpload`, `JmapBlobDownload` and `JmapEventSource`, without the port. io-http 0.6 writes it from the URL, port included when it is not the default, when the request sets none (its rfc9112/request.rs), so the five lines are gone. Not breaking.

JMAP-16: `JmapSession::account_for` (rfc8620/session.rs) returns the primary account of a capability or `JmapSessionAccountError`, on a missing and on an empty id, and the 35 constructors that defaulted to an empty `accountId` go through it, each error enum gaining an `Account` variant. **Breaking**: 35 exhaustive enums gained a variant. The second half of the finding, an account id per request, is not done: several constructors take no options struct, so where it lives is the maintainer's call. `primary_account_id_for` and the never-constructed `JmapSessionGetError::NoPrimaryMailAccount` are left as they were.

JMAP-17: `JmapEmailSetArgs::create` (rfc8621/email/set.rs) is serialised through `serialize_create`, which drops the `null` members at every depth. `JmapEmail` keeps its serialisation, so what a consumer prints for an email it read does not move. Whether a real server refused the nulls was never verified; the serialisation was certain. Not breaking.

JMAP-20: `JmapClientStd::session_get` (client.rs) follows a redirect on the same stream when it keeps the origin and the server keeps the connection, five hops at most, the policy the Android bridge already had, and returns `UnexpectedRedirect` otherwise. This reverses, for that one safe case, the stance the README stated ("surfacing redirects to the caller instead of following them"), reworded. The coroutine still follows nothing. A redirect to the root path is discovered again through `/.well-known/jmap`, `JmapSessionGet::new` treating a root path so; the hop limit bounds it. Not breaking.

Capabilities moved: requests (new: "The Host header comes from the URL", "A method is addressed to a named account", "An Email create omits what is unset", "An event source subscription can resume"), responses (new: "Event source frames carry their id", "Session discovery follows a redirect that stays put").

Verified, each test red before its fix: against the unfixed code the five request heads held a `Host` with no port, `subscribe_url` gave `types=&closeafter=state&ping=15`, the frame `event: other` completed with `DecodeFrame(InvalidJson)`, the yield of a frame with `id: evt-1` held no `evt-1`, `JmapEmailGet::new` accepted a session with no account, the create serialised every unset property as `null`, and `session_get` returned `UnexpectedRedirect` on a same-origin 301 and stopped at the first of six. Tests: `request_host_keeps_the_port` in the five files, `subscribe_url_preserves_existing_query` (its expectation moved from `types=` to `types=*`), `frame_of_another_event_is_ignored`, `frame_yields_its_event_id`, `last_event_id_is_sent_and_checked`, `account_for_wants_a_named_non_empty_account`, `constructor_without_a_primary_account_fails`, `create_omits_unset_properties`, and the four `session_get_*` tests of client.rs over a scripted stream, two of them controls (a cross-origin redirect, a redirect closing the connection).
