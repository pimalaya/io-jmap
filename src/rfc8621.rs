//! RFC 8621: JMAP for Mail.

pub mod email;
pub mod email_submission;
pub mod identity;
pub mod mailbox;
pub mod thread;
pub mod vacation_response;

/// JMAP for Mail capability (RFC 8621).
pub const JMAP_MAIL_CAPABILITY: &str = "urn:ietf:params:jmap:mail";
