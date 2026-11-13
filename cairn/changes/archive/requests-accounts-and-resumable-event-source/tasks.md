---
cairn: tasks
change: requests-accounts-and-resumable-event-source
---

# Tasks

- [x] Tests failing first: a Host without its port on the five requests, `types=` for an empty slice, a frame of another event failing the stream, a frame id nowhere in the yield, a constructor accepting a session with no account, a create full of `null`, a same-origin redirect refused
- [x] Delete the five hand-set `Host` headers
- [x] `Frame { change, id }`, `JmapEventSourceOptions::last_event_id`, `InvalidLastEventId`, `*` for no type, other events skipped
- [x] `JmapSession::account_for`, `JmapSessionAccountError`, an `Account` variant on the 35 error enums
- [x] `JmapEmailSetArgs::create` serialised without `null` members
- [x] `JmapClientStd::session_get` follows a same-origin, kept-alive redirect, five at most
- [x] Control tests: a cross-origin redirect, a redirect closing the connection and a sixth redirect still fail; the vacation coroutines fall back to the mail account
- [x] README, CHANGELOG, spec, log
- [ ] An account id per request (where it lives is undecided)
- [ ] `JmapSessionGetError::NoPrimaryMailAccount` is still never constructed
