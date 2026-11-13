---
cairn: log
change: method-errors-and-query-order
landed: 2026-11-12
---

# Method errors keep their type, chained queries keep their order

Two more findings of the Himalaya live test on Fastmail (2026-11-12).

`JmapMethodError` dropped what it did not model: `Email/copy` from an unknown account printed `JMAP unknown error` where Fastmail said `fromAccountNotFound: Not a valid accountId`, and `invalidArguments` lost the `arguments` Fastmail names (`filter/inMailbox`). The catch-all is now an untagged `Other { type_name, description }`, as the SetErrors got the same day (2026-11-12-set-errors-kept), `InvalidArguments` gained `arguments`, and the 25 `error` response parsers fall back through `JmapMethodError::unreadable`, which keeps the parse failure as description. **Breaking**.

`Mailbox/query --sort name` came back in `Mailbox/get` order: the chained coroutines returned the `/get` list as the server ordered it, which RFC 8620 §5.1 leaves free. The five chained query coroutines now read the query `ids` and sort the list by them through `rfc8620::query::order_by_ids`.

Capabilities moved: responses (new: "Method errors keep what the server named", "A chained query keeps the query order").

Verified: `an_unmodelled_type_keeps_what_the_server_said` and `invalid_arguments_name_the_arguments` (src/rfc8620/error.rs), `mailbox_query_keeps_the_query_order` and `email_query_keeps_the_query_order` (tests/rfc8621.rs) fail against 0.7.0 and pass now; `cargo clippy --all-features --all-targets` clean, `cargo test --all-features` green, builds with no default features and with `client` alone.
