//! JMAP for Mail: VacationResponse (RFC 8621 §8).

pub mod get;
pub mod set;
mod types;
mod utils;

#[doc(inline)]
pub use types::*;
#[doc(inline)]
pub use utils::*;
