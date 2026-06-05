//! RFC 8621: JMAP for Mail.

pub mod email;
pub mod email_submission;
pub mod identity;
pub mod mailbox;
pub mod thread;
mod utils;
pub mod vacation_response;

#[doc(inline)]
pub use utils::*;
