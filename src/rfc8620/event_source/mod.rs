//! JMAP Event Source push channel (RFC 8620 §7.2 & §7.2.1).
//!
//! A streaming GET against
//! [`JmapSession::event_source_url`](crate::rfc8620::JmapSession::event_source_url)
//! yields W3C SSE frames carrying [`JmapStateChange`] payloads.

pub mod subscribe;
mod types;
mod utils;

#[doc(inline)]
pub use types::*;
#[doc(inline)]
pub use utils::*;
