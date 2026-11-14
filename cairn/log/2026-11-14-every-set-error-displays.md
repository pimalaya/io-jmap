---
cairn: log
change: every-set-error-displays
landed: 2026-11-14
---

# Every set error displays

Closes the gap the 2026-11-14 a-get-keeps-what-it-misses entry left open: `JmapIdentitySetItemError` (rfc8621/identity/set.rs), `JmapAddressBookSetItemError` (rfc9610/address_book/set.rs) and `JmapContactCardCopyItemError` (rfc9610/contact_card/copy.rs) had no `Display` although the responses spec says every per-object SetError enum displays as `JMAP <type>[: <description>]`. They gained one in the shape of `JmapEmailCopyItemError` and `JmapMailboxSetItemError`: each modelled type through `write_described`, `invalidProperties` through `write_properties`, `alreadyExists` as `JMAP alreadyExists (existing id <id>)[: <description>]`, and `Other` through `write_described` with its `type_name`, empty when the server named none. The other seven enums the requirement names already displayed. Not breaking.

No capability moved: responses already says so ("Set errors keep what the server named").

Verified: the new tests do not build against the old code (no `Display`, so no `to_string`). Tests: `a_refusal_reads_as_its_type_and_description` and `an_unmodelled_error_keeps_its_type_and_description` in rfc8621/identity/set.rs and rfc9610/address_book/set.rs; `already_exists_keeps_the_id_the_server_names` and `an_unmodelled_error_keeps_its_type_and_description` in rfc9610/contact_card/copy.rs.
