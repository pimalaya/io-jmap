//! JMAP for Contacts: ContactCard (RFC 9610 §3).

mod types;
#[doc(inline)]
pub use types::*;

pub mod changes;
pub mod copy;
pub mod get;
pub mod query;
pub mod set;
