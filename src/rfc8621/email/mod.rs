//! JMAP for Mail: Email (RFC 8621 §4).

pub mod changes;
pub mod copy;
pub mod get;
pub mod import;
pub mod parse;
pub mod query;
pub mod set;
mod types;
mod utils;

#[doc(inline)]
pub use types::*;
#[doc(inline)]
pub use utils::*;
