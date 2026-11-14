---
cairn: tasks
change: a-get-keeps-what-it-misses
---

# Tasks

- [ ] `not_found` on `JmapEmailQueryOutput`, `JmapMailboxQueryOutput`, `JmapEmailSubmissionQueryOutput`, `JmapCalendarEventQueryOutput`, `JmapContactCardQueryOutput`
- [ ] `JmapUnparsed`; `unparsed` on `JmapEmailGetOutput`, `JmapEmailQueryOutput`, `JmapMailboxGetOutput`, `JmapMailboxQueryOutput`, each `list` read object by object
- [ ] `Forbidden`, `OverQuota`, `TooLarge`, `RateLimit` on the Email set, copy and import errors and on the Mailbox set error, with their display
- [ ] `invalid_recipients` and `max_recipients` on the submission errors
- [ ] Tests: a query whose get misses an id; a get with one malformed object among two; each new SetError type; `invalidRecipients` with and without its list
- [ ] cairn/spec/responses.md: fold the delta; CHANGELOG; the release left to the owner; log; status landed
