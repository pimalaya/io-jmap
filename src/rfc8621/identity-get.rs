//! I/O-free coroutine for the `Identity/get` method (RFC 8621 §6.3).

use alloc::{string::String, vec, vec::Vec};

use secrecy::SecretString;
use thiserror::Error;

use crate::coroutine::*;
use crate::{
    rfc8620::{get::*, session::JmapSession},
    rfc8621::{capabilities, identity::Identity},
};

/// Errors that can occur during the coroutine progression.
#[derive(Debug, Error)]
pub enum JmapIdentityGetError {
    #[error("JMAP Identity/get error: {0}")]
    Get(#[from] JmapGetError),
}

/// Successful terminal output of [`JmapIdentityGet`].
#[derive(Clone, Debug)]
pub struct JmapIdentityGetOutput {
    pub identities: Vec<Identity>,
    pub not_found: Vec<String>,
    pub new_state: String,
    pub keep_alive: bool,
}

/// I/O-free coroutine for the JMAP `Identity/get` method.
///
/// Fetches sender identity objects. Pass `ids: None` to fetch all identities.
pub struct JmapIdentityGet {
    get: JmapGet<Identity>,
}

impl JmapIdentityGet {
    /// Creates a new coroutine.
    pub fn new(
        session: &JmapSession,
        http_auth: &SecretString,
        ids: Option<Vec<String>>,
    ) -> Result<Self, JmapIdentityGetError> {
        let account_id = session
            .primary_accounts
            .get(capabilities::MAIL)
            .cloned()
            .unwrap_or_default();
        let api_url = &session.api_url;

        Ok(Self {
            get: JmapGet::new(
                account_id,
                http_auth,
                api_url,
                "Identity/get",
                vec![
                    capabilities::CORE.into(),
                    capabilities::MAIL.into(),
                    capabilities::SUBMISSION.into(),
                ],
                ids,
                None,
            )?,
        })
    }
}

impl JmapCoroutine for JmapIdentityGet {
    type Yield = JmapYield;
    type Return = Result<JmapIdentityGetOutput, JmapIdentityGetError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> JmapCoroutineState<Self::Yield, Self::Return> {
        match self.get.resume(arg) {
            JmapCoroutineState::Complete(Ok(JmapGetOutput {
                list,
                not_found,
                state,
                keep_alive,
            })) => JmapCoroutineState::Complete(Ok(JmapIdentityGetOutput {
                identities: list,
                not_found,
                new_state: state,
                keep_alive,
            })),
            JmapCoroutineState::Complete(Err(err)) => JmapCoroutineState::Complete(Err(err.into())),
            JmapCoroutineState::Yielded(y) => JmapCoroutineState::Yielded(y),
        }
    }
}
