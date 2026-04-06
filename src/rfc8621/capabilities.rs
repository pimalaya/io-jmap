//! JMAP for Mail capability URNs (RFC 8621).

/// Re-exported for convenience so rfc8621 callers need only one import.
pub use crate::rfc8620::session::capabilities::CORE;

/// JMAP for Mail capability (RFC 8621).
pub const MAIL: &str = "urn:ietf:params:jmap:mail";
/// JMAP for Mail Submission capability (RFC 8621).
pub const SUBMISSION: &str = "urn:ietf:params:jmap:submission";
/// JMAP for Vacation Response capability (RFC 8621).
pub const VACATION_RESPONSE: &str = "urn:ietf:params:jmap:vacationresponse";
