//! JMAP for Contacts: AddressBook (RFC 9610 §2).

mod types;
#[doc(inline)]
pub use types::*;

pub mod changes;
pub mod get;
pub mod set;
