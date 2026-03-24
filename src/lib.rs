#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
//! I/O-free coroutines for the JMAP protocol.
//!
//! This crate implements the JMAP protocol (RFC 8620) and JMAP for
//! Mail (RFC 8621) as a set of I/O-free coroutines that emit
//! [`io_stream::io::StreamIo`] requests for the caller to handle.
//!
//! The coroutines are built on top of [`io_http`]'s [`SendHttp`]
//! coroutine — JMAP is an HTTP-based protocol, not a raw TCP stream
//! protocol.
//!
//! [`SendHttp`]: io_http::v1_1::coroutines::send::SendHttp

pub mod context;
pub mod coroutines;
pub mod types;
