//! # Generic coroutine driver
//!
//! Every standard-shape coroutine in this crate exposes the same loop
//! contract: produce some bytes to write, ask for some bytes to read,
//! or terminate with success or failure. The [`JmapCoroutine`] trait
//! unifies that contract behind a single method so a generic driver
//! ([`JmapClientStd::run`]) can advance any coroutine without macros.
//!
//! Coroutines whose progression yields a redirect intermediate variant
//! ([`JmapSessionGet`](crate::rfc8620::session_get::JmapSessionGet),
//! [`JmapBlobDownload`](crate::rfc8620::blob_download::JmapBlobDownload),
//! [`JmapBlobUpload`](crate::rfc8620::blob_upload::JmapBlobUpload)) stay
//! outside this trait and keep their own per-coroutine `Result` enum.
//!
//! [`JmapClientStd::run`]: crate::client::JmapClientStd::run

use alloc::vec::Vec;

/// State yielded by a [`JmapCoroutine`] resume.
///
/// Single generic enum so a generic driver can pattern match on
/// progression without naming a per-coroutine `Result` type.
#[derive(Debug)]
pub enum JmapCoroutineState<T, E> {
    /// Coroutine terminated successfully with this payload.
    Done(T),
    /// Caller should read more bytes from the socket and feed them
    /// back on the next resume.
    WantsRead,
    /// Caller should write these bytes to the socket; the next resume
    /// typically takes `None`.
    WantsWrite(Vec<u8>),
    /// Coroutine terminated with this error.
    Err(E),
}

/// Standard-shape JMAP coroutine: anything whose progression maps onto
/// [`JmapCoroutineState`].
///
/// `resume` is the single source of truth: each implementor's body
/// returns [`JmapCoroutineState::Done`] / [`WantsRead`] /
/// [`WantsWrite`] / [`Err`] directly. [`JmapClientStd::run`] drives
/// any [`JmapCoroutine`] to completion against a blocking stream;
/// downstream code can write its own driver against the same trait.
///
/// [`JmapClientStd::run`]: crate::client::JmapClientStd::run
/// [`WantsRead`]: JmapCoroutineState::WantsRead
/// [`WantsWrite`]: JmapCoroutineState::WantsWrite
/// [`Err`]: JmapCoroutineState::Err
pub trait JmapCoroutine {
    /// Payload yielded on terminal success.
    type Output;
    /// Error yielded on terminal failure.
    type Error;

    /// Advances the coroutine one step.
    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Output, Self::Error>;
}
