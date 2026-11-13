---
cairn: tasks
change: secrets-nullable-patches-and-raw-responses
---

# Tasks

- [x] Tests failing first: the `Debug` of the push arguments showing the URL and the `auth` secret, the library's log records showing a password and a token, 46029 allocations to decode 1000 emails
- [x] `SecretString` for `auth`, a hand-written `Debug` for `JmapPushSubscriptionCreate`, five traces reduced to scheme, host and path
- [x] `Option<Option<T>>` for `parent_id`, `role`, `reply_to`, `bcc`, `description`, `share_with`, with tests of the `null` they now write
- [x] `raw_value` feature of serde_json, `Box<RawValue>` in `JmapResponse`, one `from_str` per method response in the 20 coroutine files
- [x] tests/alloc.rs, a counting allocator alone in its binary; tests/logs.rs, a capturing logger alone in its binary
- [x] CHANGELOG, spec, log
- [ ] Request arguments serialised straight to bytes
- [ ] A settable body limit for API calls
- [ ] Blob upload and download as streams
