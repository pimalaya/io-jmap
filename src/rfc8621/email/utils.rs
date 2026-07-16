//! JMAP Email helpers: standard keyword constants (RFC 8621 §4.1.1).

/// The email has been read.
pub const JMAP_KEYWORD_SEEN: &str = "$seen";

/// The email has been flagged for follow-up.
pub const JMAP_KEYWORD_FLAGGED: &str = "$flagged";

/// The email has been replied to.
pub const JMAP_KEYWORD_ANSWERED: &str = "$answered";

/// The email is a draft.
pub const JMAP_KEYWORD_DRAFT: &str = "$draft";
