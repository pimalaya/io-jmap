# Contributing guide

Thank you for investing your time in contributing to I/O JMAP.

Whether you are a human or an AI agent, read these in order before touching the code:

1. the [Pimalaya README](https://github.com/pimalaya) for what the project is and how its repositories stack;
2. the [Pimalaya CONTRIBUTING](https://github.com/pimalaya/.github/blob/master/CONTRIBUTING.md) guide, which chains to the shared architecture and guidelines;
3. the inline header documentation, starting with src/lib.rs: it is the architecture document of this crate;
4. the docs/ folder for the development history and living plans.

Everything below documents only what differs from the Pimalaya standards.

## Live provider tests

Next to the in-memory unit tests, two ignored integration tests exercise the full coroutine flow against real JMAP servers.

The Stalwart test runs against a local instance: bootstrap it with tests/stalwart.sh (provisions the pimalaya.org domain and a test user on port 8080), then run:

```sh
cargo test --test stalwart -- --include-ignored
```

The Fastmail test runs over HTTPS against a real account and requires two environment variables, FASTMAIL_EMAIL and FASTMAIL_API_TOKEN (the full Authorization header value, Bearer included):

```sh
FASTMAIL_API_TOKEN="Bearer <token>" FASTMAIL_EMAIL="user@fastmail.com" cargo test --test fastmail -- --include-ignored
```

Both flows create a test mailbox, import an email, exercise query/get/set/thread, and clean everything up on success.
