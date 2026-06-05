//! JMAP for Mail: EmailSubmission (RFC 8621 §7).

mod types;
mod utils;
#[doc(inline)]
pub use types::*;
#[doc(inline)]
pub use utils::*;

pub mod cancel;
pub mod get;
pub mod query;
pub mod set;
