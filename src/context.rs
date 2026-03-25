//! JMAP session context.

use secrecy::SecretString;

use crate::types::session::JmapSession;

/// JMAP session context.
///
/// Holds the session state needed across JMAP requests. Unlike IMAP,
/// JMAP is stateless HTTP — there is no persistent binary stream
/// state. The context tracks the discovered session object, the
/// resolved account ID, and the bearer token.
#[derive(Clone, Debug, Default)]
pub struct JmapContext {
    /// The discovered JMAP session object (RFC 8620 §2).
    ///
    /// Populated by [`GetJmapSession`] during connection setup. Provides
    /// `api_url`, `upload_url`, `download_url`, and account information.
    ///
    /// [`GetJmapSession`]: crate::coroutines::get_session::GetJmapSession
    pub session: Option<JmapSession>,

    /// The JMAP account ID to use for all method calls.
    ///
    /// Derived from `session.primary_accounts` for the mail capability.
    /// Set automatically by [`GetJmapSession`].
    ///
    /// [`GetJmapSession`]: crate::coroutines::get_session::GetJmapSession
    pub account_id: Option<String>,

    /// The current server state string for incremental sync.
    ///
    /// Updated when processing responses that include a `newState` field.
    pub state: Option<String>,

    /// The bearer token for the `Authorization` header.
    ///
    /// Stored as [`SecretString`] to prevent accidental logging.
    pub http_auth: Option<SecretString>,
}

impl JmapContext {
    /// Creates a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new context with a bearer token pre-configured.
    pub fn with_http_auth(auth: impl Into<SecretString>) -> Self {
        Self {
            http_auth: Some(auth.into()),
            ..Self::default()
        }
    }

    /// Returns the JMAP API URL from the discovered session, if available.
    pub fn api_url(&self) -> Option<&url::Url> {
        self.session.as_ref().map(|s| &s.api_url)
    }

    /// Returns the primary account ID for the given capability URN.
    ///
    /// Falls back to `account_id` (the mail primary account) if the session
    /// does not advertise a separate primary account for the capability.
    pub fn account_id_for(&self, capability: &str) -> Option<String> {
        if let Some(session) = &self.session {
            if let Some(id) = session.primary_accounts.get(capability) {
                return Some(id.clone());
            }
        }
        self.account_id.clone()
    }
}
