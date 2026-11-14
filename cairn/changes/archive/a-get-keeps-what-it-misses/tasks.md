---
cairn: tasks
change: a-get-keeps-what-it-misses
---

# Tasks

- [x] `not_found` on `JmapEmailQueryOutput`, `JmapMailboxQueryOutput`, `JmapEmailSubmissionQueryOutput`, `JmapCalendarEventQueryOutput`, `JmapContactCardQueryOutput`
- [x] `JmapUnparsed`; `unparsed` on `JmapEmailGetOutput`, `JmapEmailQueryOutput`, `JmapMailboxGetOutput`, `JmapMailboxQueryOutput`, each `list` read object by object
- [x] `Forbidden`, `OverQuota`, `TooLarge`, `RateLimit` on the Email set, copy and import errors and on the Mailbox set error, with their display
- [x] `invalid_recipients` and `max_recipients` on the submission errors
- [x] Tests: a query whose get misses an id; a get with one malformed object among two; each new SetError type; `invalidRecipients` with and without its list
- [x] cairn/spec/responses.md: fold the delta; CHANGELOG; log; status landed
- [ ] The release, left to the owner
