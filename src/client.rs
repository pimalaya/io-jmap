//! Standard, blocking JMAP client.
//!
//! Wraps a single boxed stream plus the bearer token and discovered
//! [`JmapSession`], with one method per common coroutine. [`JmapClientStd::new`]
//! takes a pre-connected stream; with one of the TLS features enabled,
//! [`JmapClientStd::connect`] handles `https://` URLs end-to-end.
//!
//! Run [`JmapClientStd::session_get`] once after construction to populate the
//! session; subsequent calls resolve `accountId` and `apiUrl` from it.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_jmap::client::JmapClientStd;
//! use pimalaya_stream::tls::Tls;
//! use secrecy::SecretString;
//! use url::Url;
//!
//! let url: Url = "https://api.example.com/jmap/session/".parse().unwrap();
//! let auth = SecretString::from("Bearer xyz");
//!
//! let mut client = JmapClientStd::connect(&url, &Tls::default(), auth).unwrap();
//! let session = client.session_get(&url).unwrap();
//!
//! println!("logged in as {}", session.username);
//! ```

use core::{any::Any, fmt, time::Duration};

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
use alloc::string::ToString;
use alloc::{boxed::Box, collections::BTreeMap, string::String, vec, vec::Vec};

use std::io::{self, Read, Write};

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
use pimalaya_stream::{std::stream::StreamStd, tls::Tls};
use secrecy::SecretString;
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    rfc8620::{
        JmapRequest, JmapResponse, JmapSession,
        blob_download::*,
        blob_upload::*,
        changes::JmapChangesOutput,
        coroutine::JmapRedirectYield,
        push_subscription::{get::*, set::*},
        send::*,
        session_get::*,
    },
    rfc8621::{
        email::{
            JmapEmailCopyArgs, JmapEmailImportArgs, changes::*, copy::*, get::*, import::*,
            parse::*, query::*, set::*,
        },
        email_submission::{cancel::*, get::*, query::*, set::*, *},
        identity::{get::*, set::*},
        mailbox::{changes::*, get::*, query::*, set::*},
        thread::{changes::*, get::*},
        vacation_response::{get::*, set::*, *},
    },
};

/// Errors returned by [`JmapClientStd`].
#[derive(Debug, Error)]
pub enum JmapClientStdError {
    #[error(transparent)]
    Send(#[from] JmapSendError),
    #[error(transparent)]
    SessionGet(#[from] JmapSessionGetError),
    #[error(transparent)]
    BlobUpload(#[from] JmapBlobUploadError),
    #[error(transparent)]
    BlobDownload(#[from] JmapBlobDownloadError),

    #[error(transparent)]
    PushSubscriptionGet(#[from] JmapPushSubscriptionGetError),
    #[error(transparent)]
    PushSubscriptionSet(#[from] JmapPushSubscriptionSetError),

    #[error(transparent)]
    MailboxGet(#[from] JmapMailboxGetError),
    #[error(transparent)]
    MailboxQuery(#[from] JmapMailboxQueryError),
    #[error(transparent)]
    MailboxSet(#[from] JmapMailboxSetError),
    #[error(transparent)]
    MailboxChanges(#[from] JmapMailboxChangesError),

    #[error(transparent)]
    EmailGet(#[from] JmapEmailGetError),
    #[error(transparent)]
    EmailQuery(#[from] JmapEmailQueryError),
    #[error(transparent)]
    EmailSet(#[from] JmapEmailSetError),
    #[error(transparent)]
    EmailChanges(#[from] JmapEmailChangesError),
    #[error(transparent)]
    JmapEmailCopyArgs(#[from] JmapEmailCopyError),
    #[error(transparent)]
    JmapEmailImportArgs(#[from] JmapEmailImportError),
    #[error(transparent)]
    EmailParse(#[from] JmapEmailParseError),

    #[error(transparent)]
    ThreadGet(#[from] JmapThreadGetError),
    #[error(transparent)]
    ThreadChanges(#[from] JmapThreadChangesError),

    #[error(transparent)]
    IdentityGet(#[from] JmapIdentityGetError),
    #[error(transparent)]
    IdentitySet(#[from] JmapIdentitySetError),

    #[error(transparent)]
    EmailSubmissionGet(#[from] JmapEmailSubmissionGetError),
    #[error(transparent)]
    EmailSubmissionQuery(#[from] JmapEmailSubmissionQueryError),
    #[error(transparent)]
    EmailSubmissionSet(#[from] JmapEmailSubmissionSetError),
    #[error(transparent)]
    EmailSubmissionCancel(#[from] JmapEmailSubmissionCancelError),

    #[error(transparent)]
    VacationResponseGet(#[from] JmapVacationResponseGetError),
    #[error(transparent)]
    VacationResponseSet(#[from] JmapVacationResponseSetError),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[cfg(any(
        feature = "rustls-aws",
        feature = "rustls-ring",
        feature = "native-tls"
    ))]
    #[error(transparent)]
    Tls(#[from] anyhow::Error),
    #[cfg(any(
        feature = "rustls-aws",
        feature = "rustls-ring",
        feature = "native-tls"
    ))]
    #[error("JMAP URL `{0}` has no host")]
    UrlMissingHost(String),
    #[cfg(any(
        feature = "rustls-aws",
        feature = "rustls-ring",
        feature = "native-tls"
    ))]
    #[error(
        "JMAP URL `{0}` has unsupported scheme `{1}` (expected `http`, `https`, `jmap` or `jmaps`)"
    )]
    UrlUnsupportedScheme(String, String),

    #[error("JMAP server redirected to `{0}` during a non-redirectable operation")]
    UnexpectedRedirect(Url),
    #[error("JMAP client missing session; call `session_get` first")]
    MissingSession,
}

const READ_BUFFER_SIZE: usize = 16 * 1024;

/// Default ALPN list for JMAP TLS handshakes: `["http/1.1"]` (JMAP rides on
/// HTTP/1.1). Exposed so config-driven callers can share one source of truth.
pub fn default_alpn() -> Vec<String> {
    vec![String::from("http/1.1")]
}

/// Std-blocking JMAP client wrapping a single boxed stream.
pub struct JmapClientStd {
    pub stream: Box<dyn JmapStream>,
    pub http_auth: SecretString,
    pub session: Option<JmapSession>,
}

impl JmapClientStd {
    /// Builds a client around `stream`. The caller is responsible for opening
    /// the connection (TCP, TLS handshake if needed) and for the bearer token /
    /// authorization header value.
    pub fn new<S: Read + Write + Send + 'static>(stream: S, http_auth: SecretString) -> Self {
        Self {
            stream: Box::new(stream),
            http_auth,
            session: None,
        }
    }

    /// Drives any standard-shape coroutine (`Yield = JmapYield`) against the
    /// wrapped stream until it terminates.
    ///
    /// Redirect-aware coroutines ([`JmapSessionGet`], [`JmapBlobUpload`],
    /// [`JmapBlobDownload`]) and the streaming
    /// [`JmapEventSource`](crate::rfc8620::event_source::subscribe::JmapEventSource)
    /// have their own per-method loops.
    pub fn run<C, T, E>(&mut self, mut coroutine: C) -> Result<T, JmapClientStdError>
    where
        C: JmapCoroutine<Yield = JmapYield, Return = Result<T, E>>,
        JmapClientStdError: From<E>,
    {
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut arg: Option<&[u8]> = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapCoroutineState::Complete(Ok(out)) => return Ok(out),
                JmapCoroutineState::Complete(Err(err)) => return Err(err.into()),
                JmapCoroutineState::Yielded(JmapYield::WantsRead) => {
                    let n = self.stream.read(&mut buf)?;
                    arg = Some(&buf[..n]);
                }
                JmapCoroutineState::Yielded(JmapYield::WantsWrite(bytes)) => {
                    self.stream.write_all(&bytes)?;
                    arg = None;
                }
            }
        }
    }

    /// Builds a client from a pre-connected stream and an already-discovered
    /// [`JmapSession`]. Skips [`Self::session_get`].
    pub fn from_parts<S: Read + Write + Send + 'static>(
        stream: S,
        http_auth: SecretString,
        session: JmapSession,
    ) -> Self {
        Self {
            stream: Box::new(stream),
            http_auth,
            session: Some(session),
        }
    }

    /// Connects to `url`, doing a TLS handshake for `https` / `jmaps` (plain
    /// TCP for `http` / `jmap`). ALPN comes from `tls.rustls.alpn` (see
    /// [`default_alpn`]); empty vec skips ALPN.
    #[cfg(any(
        feature = "rustls-aws",
        feature = "rustls-ring",
        feature = "native-tls"
    ))]
    pub fn connect(
        url: &Url,
        tls: &Tls,
        http_auth: SecretString,
    ) -> Result<Self, JmapClientStdError> {
        let host = url
            .host_str()
            .ok_or_else(|| JmapClientStdError::UrlMissingHost(url.to_string()))?;

        let stream = match url.scheme() {
            "http" | "jmap" => StreamStd::connect_tcp(host, url.port().unwrap_or(80))?,
            "https" | "jmaps" => StreamStd::connect_tls(host, url.port().unwrap_or(443), tls)?,
            scheme => {
                return Err(JmapClientStdError::UrlUnsupportedScheme(
                    url.to_string(),
                    scheme.to_string(),
                ));
            }
        };

        // NOTE: 5s per-read (not per-operation) timeout so the watch loop
        // polls its shutdown atomic between SSE push frames; large JMAP
        // responses keep working as long as TCP packets keep arriving.
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;

        Ok(Self {
            stream: Box::new(stream),
            http_auth,
            session: None,
        })
    }

    /// Replaces the underlying stream; useful when `apiUrl`, `uploadUrl` or
    /// `downloadUrl` resolves to a different authority than the first
    /// connection target, or after a redirect.
    pub fn set_stream<S: Read + Write + Send + 'static>(&mut self, stream: S) {
        self.stream = Box::new(stream);
    }

    /// Returns the cached session, if [`Self::session_get`] has run.
    pub fn session(&self) -> Option<&JmapSession> {
        self.session.as_ref()
    }

    /// Returns the pre-formatted HTTP `Authorization` header value.
    pub fn http_auth(&self) -> &SecretString {
        &self.http_auth
    }

    fn session_or_err(&self) -> Result<&JmapSession, JmapClientStdError> {
        self.session
            .as_ref()
            .ok_or(JmapClientStdError::MissingSession)
    }

    /// Runs [`JmapSessionGet`] and caches the discovered session.
    ///
    /// `url` is either a base URL for `/.well-known/jmap` discovery or a
    /// direct session endpoint. A 3xx response terminates with
    /// [`JmapClientStdError::UnexpectedRedirect`].
    pub fn session_get(&mut self, url: &Url) -> Result<&JmapSession, JmapClientStdError> {
        let mut coroutine = JmapSessionGet::new(&self.http_auth, url);
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut arg: Option<&[u8]> = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapCoroutineState::Complete(Ok(JmapSessionGetOutput { session, .. })) => {
                    self.session = Some(session);
                    return Ok(self.session.as_ref().unwrap());
                }
                JmapCoroutineState::Complete(Err(err)) => return Err(err.into()),
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsRead) => {
                    let n = self.stream.read(&mut buf)?;
                    arg = Some(&buf[..n]);
                }
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsWrite(bytes)) => {
                    self.stream.write_all(&bytes)?;
                    arg = None;
                }
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsRedirect { url, .. }) => {
                    return Err(JmapClientStdError::UnexpectedRedirect(url));
                }
            }
        }
    }

    /// Sends a raw JMAP request and returns the raw [`JmapResponse`]. Useful
    /// for passthrough CLIs and ad-hoc requests with custom `using`
    /// capabilities.
    // TODO: move this to one level down
    pub fn send_raw(&mut self, request: JmapRequest) -> Result<JmapResponse, JmapClientStdError> {
        let session = self.session_or_err()?;
        let coroutine = JmapSend::new(&self.http_auth, &session.api_url, request)?;
        let out = self.run(coroutine)?;
        Ok(out.response)
    }

    // ---- Blob (RFC 8620 §6) ----------------------------------------------

    /// Uploads a blob to `upload_url` (RFC 8620 §6.1). The caller must resolve
    /// the session's `uploadUrl` template (e.g. substitute `{accountId}`).
    /// A 3xx response terminates with [`JmapClientStdError::UnexpectedRedirect`].
    pub fn blob_upload(
        &mut self,
        upload_url: &Url,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<JmapBlobUploadOutput, JmapClientStdError> {
        let mut coroutine = JmapBlobUpload::new(&self.http_auth, upload_url, content_type, data);
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut arg: Option<&[u8]> = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapCoroutineState::Complete(Ok(out)) => return Ok(out),
                JmapCoroutineState::Complete(Err(err)) => return Err(err.into()),
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsRead) => {
                    let n = self.stream.read(&mut buf)?;
                    arg = Some(&buf[..n]);
                }
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsWrite(bytes)) => {
                    self.stream.write_all(&bytes)?;
                    arg = None;
                }
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsRedirect { url, .. }) => {
                    return Err(JmapClientStdError::UnexpectedRedirect(url));
                }
            }
        }
    }

    /// Downloads a blob from `download_url` (RFC 8620 §6.2). The caller must
    /// resolve the session's `downloadUrl` template. A 3xx response terminates
    /// with [`JmapClientStdError::UnexpectedRedirect`].
    pub fn blob_download(&mut self, download_url: &Url) -> Result<Vec<u8>, JmapClientStdError> {
        let mut coroutine = JmapBlobDownload::new(&self.http_auth, download_url);
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut arg: Option<&[u8]> = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapCoroutineState::Complete(Ok(out)) => return Ok(out.data),
                JmapCoroutineState::Complete(Err(err)) => return Err(err.into()),
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsRead) => {
                    let n = self.stream.read(&mut buf)?;
                    arg = Some(&buf[..n]);
                }
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsWrite(bytes)) => {
                    self.stream.write_all(&bytes)?;
                    arg = None;
                }
                JmapCoroutineState::Yielded(JmapRedirectYield::WantsRedirect { url, .. }) => {
                    return Err(JmapClientStdError::UnexpectedRedirect(url));
                }
            }
        }
    }

    // ---- PushSubscription (RFC 8620 §7.2) ----------------------------------

    /// Runs [`JmapPushSubscriptionGet`] (`PushSubscription/get`).
    pub fn push_subscription_get(
        &mut self,
        opts: JmapPushSubscriptionGetOptions,
    ) -> Result<JmapPushSubscriptionGetOutput, JmapClientStdError> {
        let coroutine =
            JmapPushSubscriptionGet::new(self.session_or_err()?, &self.http_auth, opts)?;
        self.run(coroutine)
    }

    /// Runs [`JmapPushSubscriptionSet`] (`PushSubscription/set`).
    pub fn push_subscription_set(
        &mut self,
        args: JmapPushSubscriptionSetArgs,
    ) -> Result<JmapPushSubscriptionSetOutput, JmapClientStdError> {
        let coroutine =
            JmapPushSubscriptionSet::new(self.session_or_err()?, &self.http_auth, args)?;
        self.run(coroutine)
    }

    // ---- Mailbox (RFC 8621 §2) -------------------------------------------

    /// Runs [`JmapMailboxGet`] (`Mailbox/get`).
    pub fn mailbox_get(
        &mut self,
        opts: JmapMailboxGetOptions,
    ) -> Result<JmapMailboxGetOutput, JmapClientStdError> {
        let coroutine = JmapMailboxGet::new(self.session_or_err()?, &self.http_auth, opts)?;
        self.run(coroutine)
    }

    /// Runs [`JmapMailboxQuery`] (batched `Mailbox/query` +
    /// `Mailbox/get`).
    pub fn mailbox_query(
        &mut self,
        opts: JmapMailboxQueryOptions,
    ) -> Result<JmapMailboxQueryOutput, JmapClientStdError> {
        let coroutine = JmapMailboxQuery::new(self.session_or_err()?, &self.http_auth, opts)?;
        self.run(coroutine)
    }

    /// Runs [`JmapMailboxSet`] (`Mailbox/set`).
    pub fn mailbox_set(
        &mut self,
        args: JmapMailboxSetArgs,
    ) -> Result<JmapMailboxSetOutput, JmapClientStdError> {
        let coroutine = JmapMailboxSet::new(self.session_or_err()?, &self.http_auth, args)?;
        self.run(coroutine)
    }

    /// Runs [`JmapMailboxChanges`] (`Mailbox/changes`).
    pub fn mailbox_changes(
        &mut self,
        since_state: impl Into<String>,
        opts: JmapMailboxChangesOptions,
    ) -> Result<JmapChangesOutput, JmapClientStdError> {
        let coroutine =
            JmapMailboxChanges::new(self.session_or_err()?, &self.http_auth, since_state, opts)?;
        self.run(coroutine)
    }

    // ---- Email (RFC 8621 §4) ---------------------------------------------

    /// Runs [`JmapEmailGet`] (`Email/get`).
    pub fn email_get(
        &mut self,
        ids: Vec<String>,
        opts: JmapEmailGetOptions,
    ) -> Result<JmapEmailGetOutput, JmapClientStdError> {
        let coroutine = JmapEmailGet::new(self.session_or_err()?, &self.http_auth, ids, opts)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailQuery`] (batched `Email/query` + `Email/get`).
    pub fn email_query(
        &mut self,
        opts: JmapEmailQueryOptions,
    ) -> Result<JmapEmailQueryOutput, JmapClientStdError> {
        let coroutine = JmapEmailQuery::new(self.session_or_err()?, &self.http_auth, opts)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailSet`] (`Email/set`).
    pub fn email_set(
        &mut self,
        args: JmapEmailSetArgs,
    ) -> Result<JmapEmailSetOutput, JmapClientStdError> {
        let coroutine = JmapEmailSet::new(self.session_or_err()?, &self.http_auth, args)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailChanges`] (`Email/changes`).
    pub fn email_changes(
        &mut self,
        since_state: impl Into<String>,
        opts: JmapEmailChangesOptions,
    ) -> Result<JmapChangesOutput, JmapClientStdError> {
        let coroutine =
            JmapEmailChanges::new(self.session_or_err()?, &self.http_auth, since_state, opts)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailCopy`] (`Email/copy`).
    pub fn email_copy(
        &mut self,
        from_account_id: impl Into<String>,
        emails: BTreeMap<String, JmapEmailCopyArgs>,
    ) -> Result<JmapEmailCopyOutput, JmapClientStdError> {
        let coroutine = JmapEmailCopy::new(
            self.session_or_err()?,
            &self.http_auth,
            from_account_id,
            emails,
        )?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailImport`] (`Email/import`).
    pub fn email_import(
        &mut self,
        emails: BTreeMap<String, JmapEmailImportArgs>,
    ) -> Result<JmapEmailImportOutput, JmapClientStdError> {
        let coroutine = JmapEmailImport::new(self.session_or_err()?, &self.http_auth, emails)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailParse`] (`Email/parse`).
    pub fn email_parse(
        &mut self,
        blob_ids: Vec<String>,
        opts: JmapEmailParseOptions,
    ) -> Result<JmapEmailParseOutput, JmapClientStdError> {
        let coroutine =
            JmapEmailParse::new(self.session_or_err()?, &self.http_auth, blob_ids, opts)?;
        self.run(coroutine)
    }

    // ---- Thread (RFC 8621 §3) --------------------------------------------

    /// Runs [`JmapThreadGet`] (`Thread/get`).
    pub fn thread_get(
        &mut self,
        ids: Vec<String>,
    ) -> Result<JmapThreadGetOutput, JmapClientStdError> {
        let coroutine = JmapThreadGet::new(self.session_or_err()?, &self.http_auth, ids)?;
        self.run(coroutine)
    }

    /// Runs [`JmapThreadChanges`] (`Thread/changes`).
    pub fn thread_changes(
        &mut self,
        since_state: impl Into<String>,
        opts: JmapThreadChangesOptions,
    ) -> Result<JmapChangesOutput, JmapClientStdError> {
        let coroutine =
            JmapThreadChanges::new(self.session_or_err()?, &self.http_auth, since_state, opts)?;
        self.run(coroutine)
    }

    // ---- Identity (RFC 8621 §6) ------------------------------------------

    /// Runs [`JmapIdentityGet`] (`Identity/get`).
    pub fn identity_get(
        &mut self,
        opts: JmapIdentityGetOptions,
    ) -> Result<JmapIdentityGetOutput, JmapClientStdError> {
        let coroutine = JmapIdentityGet::new(self.session_or_err()?, &self.http_auth, opts)?;
        self.run(coroutine)
    }

    /// Runs [`JmapIdentitySet`] (`Identity/set`).
    pub fn identity_set(
        &mut self,
        args: JmapIdentitySetArgs,
    ) -> Result<JmapIdentitySetOutput, JmapClientStdError> {
        let coroutine = JmapIdentitySet::new(self.session_or_err()?, &self.http_auth, args)?;
        self.run(coroutine)
    }

    // ---- EmailSubmission (RFC 8621 §7) -----------------------------------

    /// Runs [`JmapEmailSubmissionGet`] (`EmailSubmission/get`).
    pub fn email_submission_get(
        &mut self,
        opts: JmapEmailSubmissionGetOptions,
    ) -> Result<JmapEmailSubmissionGetOutput, JmapClientStdError> {
        let coroutine = JmapEmailSubmissionGet::new(self.session_or_err()?, &self.http_auth, opts)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailSubmissionQuery`] (batched
    /// `EmailSubmission/query` + `EmailSubmission/get`).
    pub fn email_submission_query(
        &mut self,
        opts: JmapEmailSubmissionQueryOptions,
    ) -> Result<JmapEmailSubmissionQueryOutput, JmapClientStdError> {
        let coroutine =
            JmapEmailSubmissionQuery::new(self.session_or_err()?, &self.http_auth, opts)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailSubmissionSet`] (`EmailSubmission/set`).
    pub fn email_submission_set(
        &mut self,
        submissions: BTreeMap<String, JmapEmailSubmissionCreate>,
    ) -> Result<JmapEmailSubmissionSetOutput, JmapClientStdError> {
        let coroutine =
            JmapEmailSubmissionSet::new(self.session_or_err()?, &self.http_auth, submissions)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailSubmissionCancel`] (`EmailSubmission/set` with
    /// `undoStatus: "canceled"`).
    pub fn email_submission_cancel(
        &mut self,
        ids: Vec<String>,
    ) -> Result<JmapEmailSubmissionCancelOutput, JmapClientStdError> {
        let coroutine =
            JmapEmailSubmissionCancel::new(self.session_or_err()?, &self.http_auth, ids)?;
        self.run(coroutine)
    }

    // ---- VacationResponse (RFC 8621 §8) ----------------------------------

    /// Runs [`JmapVacationResponseGet`]; returns the singleton, if any.
    pub fn vacation_response_get(
        &mut self,
    ) -> Result<Option<JmapVacationResponse>, JmapClientStdError> {
        let coroutine = JmapVacationResponseGet::new(self.session_or_err()?, &self.http_auth)?;
        Ok(self.run(coroutine)?.vacation_response)
    }

    /// Runs [`JmapVacationResponseSet`]; returns the updated singleton if the
    /// server echoed it back.
    pub fn vacation_response_set(
        &mut self,
        patch: JmapVacationResponseUpdate,
    ) -> Result<Option<JmapVacationResponse>, JmapClientStdError> {
        let coroutine =
            JmapVacationResponseSet::new(self.session_or_err()?, &self.http_auth, patch)?;
        Ok(self.run(coroutine)?.updated)
    }
}

impl fmt::Debug for JmapClientStd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JmapClientStd")
            .field("http_auth", &self.http_auth)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

/// Erased stream the client can drive: auto-implemented for any blocking
/// `Read + Write + Send + 'static`. `Send` flows through the `Box<dyn …>` so
/// `JmapClientStd` can move between worker threads; [`Self::as_any_mut`] lets
/// specialized callers downcast back to the concrete stream.
pub trait JmapStream: Read + Write + Send + Any {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Read + Write + Send + Any> JmapStream for T {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
