#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! # io-jmap
//!
//! I/O-free JMAP client coroutines built on io-http: every network
//! exchange is a resumable state machine emitting read and write
//! requests instead of performing I/O itself. The caller owns the
//! socket and pumps the coroutine with the bytes it read, whatever the
//! runtime (blocking, async, in-memory tests). The `client` feature
//! ships a ready-made std-blocking pump for callers who just want a
//! working client.
//!
//! ## Layout: one folder per RFC
//!
//! The source tree mirrors how the JMAP specification itself is
//! split, one module per RFC. [`rfc8620`] implements the core
//! protocol: the session, request and response objects, the generic
//! `Foo/get`, `Foo/set`, `Foo/query`, `Foo/changes` and
//! `Foo/queryChanges` coroutines every data type builds on, the blob
//! upload and download coroutines, plus the two push channels
//! (PushSubscription and the SSE-based Event Source). [`rfc8621`]
//! covers JMAP for Mail: Mailbox, Thread, Email, Identity,
//! EmailSubmission and VacationResponse, each folder wrapping the
//! generic core coroutines with the mail capability and its own data
//! types. [`rfc9610`] covers JMAP for Contacts: AddressBook and
//! ContactCard, where the JSContact payload stays raw JSON, converting
//! it being out of scope.
//!
//! Two modules span the RFC modules and therefore live at the crate
//! root: [`coroutine`] defines the coroutine contract every state
//! machine implements, and the optional [`client`] module (`client`
//! feature) is the std-blocking pump: a light client wrapping any
//! stream you opened yourself, or a full client opening the TCP/TLS
//! connection itself when one of the TLS features is enabled.
//!
//! ## The coroutine contract
//!
//! Every coroutine implements [`coroutine::JmapCoroutine`]: a resume
//! method taking the bytes read since the last step and returning
//! either an intermediate yield or a terminal completion. Standard
//! coroutines yield the shared read/write requests of
//! [`coroutine::JmapYield`]; richer coroutines declare their own yield
//! type, like the redirect-aware session and blob coroutines surfacing
//! 3xx responses to the caller instead of following them, or the
//! streaming Event Source coroutine yielding one push frame at a
//! time. Completion carries a per-coroutine output or error; the
//! [`jmap_try`] macro chains an inner coroutine step inside an outer
//! resume, re-yielding and short-circuiting like the question mark
//! operator.
//!
//! ## Conventions
//!
//! The crate is no_std with alloc; std only enters behind the `client`
//! feature. Every public item carries the bare `Jmap` prefix, the
//! protocol not being version-scoped. Logging follows the library
//! rules: state changes at debug level, in-process steps and data
//! dumps at trace level.

extern crate alloc;
#[cfg(feature = "client")]
extern crate std;

#[cfg(feature = "client")]
pub mod client;
pub mod coroutine;
pub mod rfc8620;
pub mod rfc8621;
pub mod rfc9610;
