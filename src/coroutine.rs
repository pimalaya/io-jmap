//! # Generator-shape coroutine driver
//!
//! Mirrors the shape of `core::ops::Coroutine`: a `Yield` associated
//! type for intermediate progress, a `Return` associated type for
//! terminal output, and a two-variant [`JmapCoroutineState`]
//! (`Yielded` / `Complete`).
//!
//! Each coroutine declares its own `Yield` enum mixing socket I/O step
//! requests with any extra intermediate variants (e.g.
//! [`JmapRedirectYield::WantsRedirect`]). Most JMAP coroutines pick
//! the standard [`JmapYield`] directly; only coroutines that need
//! extra variants declare their own.
//!
//! [`JmapClientStd::run`] drives any standard-Yield coroutine to
//! completion against a blocking stream; coroutines that need extra
//! Yield variants get their own per-method client loops.
//!
//! [`JmapClientStd::run`]: crate::client::JmapClientStd::run
//! [`JmapRedirectYield::WantsRedirect`]: crate::rfc8620::redirect::JmapRedirectYield::WantsRedirect

use alloc::vec::Vec;

/// State yielded by a [`JmapCoroutine::resume`] step.
///
/// Two-variant by design (matches std's `core::ops::CoroutineState`):
/// any further variation lives inside the per-coroutine `Yield` type.
#[derive(Debug)]
pub enum JmapCoroutineState<Y, R> {
    /// Intermediate yield. The driver reacts to `Y` (do I/O, follow a
    /// redirect, …) and resumes the coroutine again.
    Yielded(Y),
    /// Terminal yield. By convention `R = Result<Output, Error>`.
    Complete(R),
}

/// Standard-shape JMAP coroutine.
///
/// Implementors own their internal state machine and declare their
/// per-step `Yield` plus a terminal `Return`. The driver pumps I/O
/// based on the `Yield` variant and resumes until `Complete`.
pub trait JmapCoroutine {
    /// Intermediate value handed back on every step. Per-coroutine:
    /// each implementor picks exactly the variants it needs (socket
    /// I/O, redirects, …).
    type Yield;
    /// Terminal value. By convention `Result<Output, Error>`; the "ok"
    /// arm carries the operation's final output, the "error" arm
    /// carries the cause.
    type Return;

    /// Advances the coroutine one step.
    ///
    /// Pass [`None`] when there is no data to provide (initial call or
    /// after the previous yield was [`JmapYield::WantsWrite`]). Pass
    /// `Some(data)` with bytes read from the socket after a
    /// [`JmapYield::WantsRead`]. Pass `Some(&[])` to signal EOF.
    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return>;
}

/// Standard I/O-only Yield. Pick `type Yield = JmapYield` for any
/// coroutine that only needs to read or write socket bytes.
#[derive(Debug)]
pub enum JmapYield {
    /// Driver should read more bytes from the socket and feed them
    /// back on the next resume.
    WantsRead,
    /// Driver should write these bytes to the socket; the next resume
    /// typically takes `None`.
    WantsWrite(Vec<u8>),
}
