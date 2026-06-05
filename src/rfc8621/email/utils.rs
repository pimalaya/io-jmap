//! JMAP Email helpers: standard keyword constants.

pub mod keywords {
    /// The email has been read.
    pub const SEEN: &str = "$seen";
    /// The email has been flagged for follow-up.
    pub const FLAGGED: &str = "$flagged";
    /// The email has been replied to.
    pub const ANSWERED: &str = "$answered";
    /// The email is a draft.
    pub const DRAFT: &str = "$draft";
}
