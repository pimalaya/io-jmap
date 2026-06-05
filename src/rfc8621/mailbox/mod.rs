//! JMAP for Mail: Mailbox (RFC 8621 §2).

mod types;
#[doc(inline)]
pub use types::*;

pub mod changes;
pub mod get;
pub mod query;
pub mod set;
