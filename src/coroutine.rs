//! Generator-shape coroutine driver.
//!
//! Mirrors `core::ops::Coroutine`: a `Yield` associated type for intermediate
//! progress, a `Return` for terminal output, and a two-variant
//! [`JmapCoroutineState`].
//!
//! Most coroutines pick the standard [`JmapYield`] (I/O-only); redirect-aware
//! ones declare their own (e.g. [`JmapRedirectYield`]).
//!
//! [`JmapRedirectYield`]: crate::rfc8620::coroutine::JmapRedirectYield

use alloc::vec::Vec;

/// State yielded by a [`JmapCoroutine::resume`] step.
#[derive(Debug)]
pub enum JmapCoroutineState<Y, R> {
    /// Intermediate yield: the driver reacts and resumes.
    Yielded(Y),
    /// Terminal yield. By convention `R = Result<Output, Error>`.
    Complete(R),
}

/// Standard-shape JMAP coroutine.
pub trait JmapCoroutine {
    /// Intermediate value handed back on every step.
    type Yield;
    /// Terminal value. By convention `Result<Output, Error>`.
    type Return;

    /// Advances the coroutine one step.
    ///
    /// Pass [`None`] on the initial call or after a [`JmapYield::WantsWrite`].
    /// Pass `Some(data)` after a [`JmapYield::WantsRead`]; `Some(&[])` signals
    /// EOF.
    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return>;
}

/// Standard I/O-only Yield for coroutines that only read/write socket bytes.
#[derive(Debug)]
pub enum JmapYield {
    /// Driver should read more bytes and feed them back on the next resume.
    WantsRead,
    /// Driver should write these bytes; the next resume typically takes `None`.
    WantsWrite(Vec<u8>),
}

/// Coroutine `?`: forwards `Yielded` (via `Into`), short-circuits on
/// `Err` (via `Into`), evaluates to the inner `Ok` value.
#[macro_export]
macro_rules! jmap_try {
    ($coroutine:expr, $arg:expr $(,)?) => {
        match $crate::coroutine::JmapCoroutine::resume($coroutine, $arg) {
            $crate::coroutine::JmapCoroutineState::Yielded(y) => {
                return $crate::coroutine::JmapCoroutineState::Yielded(y.into());
            }
            $crate::coroutine::JmapCoroutineState::Complete(Err(err)) => {
                return $crate::coroutine::JmapCoroutineState::Complete(Err(err.into()));
            }
            $crate::coroutine::JmapCoroutineState::Complete(Ok(value)) => value,
        }
    };
}
