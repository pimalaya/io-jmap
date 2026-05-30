//! # Standard, blocking JMAP client
//!
//! Holds a single boxed stream (any blocking `Read + Write` impl)
//! plus the bearer token and discovered [`JmapSession`], and exposes
//! one method per common coroutine. The bare [`new`] constructor takes
//! a pre-connected stream; callers handle TCP and TLS themselves. With
//! one of the TLS feature flags enabled (`rustls-ring`, `rustls-aws`,
//! `native-tls`), [`connect`] is also available and handles `https://`
//! URLs end-to-end via
//! [`pimalaya_stream::std::stream::StreamStd`].
//!
//! After construction, the caller must run [`session_get`] once to
//! discover the JMAP session object (RFC 8620 §2). All subsequent
//! method calls use that cached session for `accountId` resolution
//! and the `apiUrl` endpoint.
//!
//! [`new`]: JmapClientStd::new
//! [`connect`]: JmapClientStd::connect
//! [`session_get`]: JmapClientStd::session_get

use core::fmt;

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
use alloc::string::ToString;
use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
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

use crate::coroutine::*;
use crate::{
    rfc8620::{
        blob_download::*, blob_upload::*, redirect::JmapRedirectYield, send::*,
        session::JmapSession, session_get::*,
    },
    rfc8621::{
        email::{EmailComparator, EmailCopy, EmailFilter, EmailImport, EmailProperty},
        email_changes::*,
        email_copy::*,
        email_get::*,
        email_import::*,
        email_parse::*,
        email_query::*,
        email_set::*,
        email_submission::*,
        email_submission_cancel::*,
        email_submission_get::*,
        email_submission_query::*,
        email_submission_set::*,
        identity_get::*,
        identity_set::*,
        mailbox::{MailboxFilter, MailboxProperty, MailboxSortComparator},
        mailbox_changes::*,
        mailbox_get::*,
        mailbox_query::*,
        mailbox_set::*,
        thread_changes::*,
        thread_get::*,
        vacation_response::*,
        vacation_response_get::*,
        vacation_response_set::*,
    },
};

const READ_BUFFER_SIZE: usize = 16 * 1024;

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
    EmailCopy(#[from] JmapEmailCopyError),
    #[error(transparent)]
    EmailImport(#[from] JmapEmailImportError),
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

/// Marker for everything the client can run against; auto-implemented
/// for any blocking `Read + Write + Send` impl. The `Send` supertrait
/// flows the auto-trait through the `Box<dyn Stream>` type erasure so
/// `JmapClientStd` can travel between threads in worker pools (e.g.
/// neverest's per-mailbox dispatch). Every concrete stream the
/// pimalaya stack hands in (`TcpStream`, `UnixStream`, rustls/native-tls
/// wrappers, `StreamStd`) is already `Send`.
pub trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send + ?Sized> Stream for T {}

/// Std-blocking JMAP client wrapping a single boxed stream.
pub struct JmapClientStd {
    pub stream: Box<dyn Stream>,
    pub http_auth: SecretString,
    pub session: Option<JmapSession>,
}

impl fmt::Debug for JmapClientStd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JmapClientStd")
            .field("http_auth", &self.http_auth)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl JmapClientStd {
    /// Drives any standard-shape coroutine (`Yield = JmapYield`,
    /// `Return = Result<Output, Error>`) against the wrapped stream
    /// until it terminates.
    ///
    /// Coroutines that need richer Yield variants
    /// ([`JmapSessionGet`], [`JmapBlobDownload`], [`JmapBlobUpload`]
    /// with [`JmapRedirectYield::WantsRedirect`], or
    /// [`JmapEventSource`](crate::rfc8620::event_source::JmapEventSource)
    /// for streaming) get their own per-method client loops; see
    /// [`Self::session_get`], [`Self::blob_upload`],
    /// [`Self::blob_download`].
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

    /// Builds a client around `stream`. The caller is responsible for
    /// opening the connection (TCP, TLS handshake if needed) and for
    /// the bearer token / authorization header value.
    pub fn new<S: Read + Write + Send + 'static>(stream: S, http_auth: SecretString) -> Self {
        Self {
            stream: Box::new(stream),
            http_auth,
            session: None,
        }
    }

    /// Builds a client from a pre-connected stream, the bearer / basic
    /// HTTP credential and an already-discovered [`JmapSession`]. Skips
    /// the [`session_get`] step; useful when an external runner has
    /// already resolved `/.well-known/jmap`.
    ///
    /// [`session_get`]: JmapClientStd::session_get
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

    /// Connects to `url` and runs the TLS handshake when the scheme is
    /// `https` or `jmaps`. `http` and `jmap` go through plain TCP.
    /// ALPN is set to `http/1.1`.
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

        Ok(Self {
            stream: Box::new(stream),
            http_auth,
            session: None,
        })
    }

    /// Replaces the underlying stream; useful when JMAP redirects to a
    /// different host, or when the session's `apiUrl`, `uploadUrl` or
    /// `downloadUrl` lives on a different authority than where the
    /// client first connected.
    pub fn set_stream<S: Read + Write + Send + 'static>(&mut self, stream: S) {
        self.stream = Box::new(stream);
    }

    /// Returns the cached session, if [`session_get`] has run.
    ///
    /// [`session_get`]: JmapClientStd::session_get
    pub fn session(&self) -> Option<&JmapSession> {
        self.session.as_ref()
    }

    /// Returns the pre-formatted HTTP `Authorization` header value.
    /// Useful when the caller has to spin up an auxiliary client (e.g.
    /// against the session's `downloadUrl` when it lives on a
    /// different authority than the `apiUrl`).
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
    /// Pass either a base URL for `/.well-known/jmap` discovery
    /// (`https://mail.example.com`) or a direct session endpoint
    /// (`https://api.example.com/jmap/session/`).
    ///
    /// A redirect terminates the call with
    /// [`JmapClientStdError::UnexpectedRedirect`]; the caller must
    /// open a new connection to the redirect target and retry.
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

    /// Sends a raw JMAP request and returns the raw [`JmapResponse`].
    /// Lower level than the per-method helpers: useful for passthrough
    /// CLIs and ad-hoc requests with custom `using` capabilities.
    pub fn send_raw(&mut self, request: JmapRequest) -> Result<JmapResponse, JmapClientStdError> {
        let session = self.session_or_err()?;
        let coroutine = JmapSend::new(&self.http_auth, &session.api_url, request)?;
        let out = self.run(coroutine)?;
        Ok(out.response)
    }

    // ---- Blob (RFC 8620 §6) ----------------------------------------------

    /// Uploads a blob to `upload_url` (RFC 8620 §6.1). The caller must
    /// resolve the session's `uploadUrl` template (e.g. substitute
    /// `{accountId}`) before passing it here.
    ///
    /// A redirect terminates the call with
    /// [`JmapClientStdError::UnexpectedRedirect`].
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

    /// Downloads a blob from `download_url` (RFC 8620 §6.2). The
    /// caller must resolve the session's `downloadUrl` template before
    /// passing it here.
    ///
    /// A redirect terminates the call with
    /// [`JmapClientStdError::UnexpectedRedirect`].
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

    // ---- Mailbox (RFC 8621 §2) -------------------------------------------

    /// Runs [`JmapMailboxGet`] (`Mailbox/get`).
    pub fn mailbox_get(
        &mut self,
        ids: Option<Vec<String>>,
        properties: Option<Vec<MailboxProperty>>,
    ) -> Result<JmapMailboxGetOutput, JmapClientStdError> {
        let coroutine =
            JmapMailboxGet::new(self.session_or_err()?, &self.http_auth, ids, properties)?;
        self.run(coroutine)
    }

    /// Runs [`JmapMailboxQuery`] (batched `Mailbox/query` +
    /// `Mailbox/get`).
    pub fn mailbox_query(
        &mut self,
        filter: Option<MailboxFilter>,
        sort: Option<Vec<MailboxSortComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
        properties: Option<Vec<MailboxProperty>>,
    ) -> Result<JmapMailboxQueryOutput, JmapClientStdError> {
        let coroutine = JmapMailboxQuery::new(
            self.session_or_err()?,
            &self.http_auth,
            filter,
            sort,
            position,
            limit,
            properties,
        )?;
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
        max_changes: Option<u64>,
    ) -> Result<crate::rfc8620::changes::JmapChangesOutput, JmapClientStdError> {
        let coroutine = JmapMailboxChanges::new(
            self.session_or_err()?,
            &self.http_auth,
            since_state,
            max_changes,
        )?;
        self.run(coroutine)
    }

    // ---- Email (RFC 8621 §4) ---------------------------------------------

    /// Runs [`JmapEmailGet`] (`Email/get`). `properties` accepts the
    /// typed [`EmailProperty`] enum; serde handles the wire-spelling
    /// rename per the enum's `rename_all = "camelCase"` annotation.
    pub fn email_get(
        &mut self,
        ids: Vec<String>,
        properties: Option<Vec<EmailProperty>>,
        fetch_text_body_values: bool,
        fetch_html_body_values: bool,
        max_body_value_bytes: u64,
    ) -> Result<JmapEmailGetOutput, JmapClientStdError> {
        let coroutine = JmapEmailGet::new(
            self.session_or_err()?,
            &self.http_auth,
            ids,
            properties,
            fetch_text_body_values,
            fetch_html_body_values,
            max_body_value_bytes,
        )?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailQuery`] (batched `Email/query` + `Email/get`).
    pub fn email_query(
        &mut self,
        filter: Option<crate::rfc8620::filter::Filter<EmailFilter>>,
        sort: Option<Vec<EmailComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<JmapEmailQueryOutput, JmapClientStdError> {
        let coroutine = JmapEmailQuery::new(
            self.session_or_err()?,
            &self.http_auth,
            filter,
            sort,
            position,
            limit,
            properties,
        )?;
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
        max_changes: Option<u64>,
    ) -> Result<crate::rfc8620::changes::JmapChangesOutput, JmapClientStdError> {
        let coroutine = JmapEmailChanges::new(
            self.session_or_err()?,
            &self.http_auth,
            since_state,
            max_changes,
        )?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailCopy`] (`Email/copy`).
    pub fn email_copy(
        &mut self,
        from_account_id: impl Into<String>,
        emails: BTreeMap<String, EmailCopy>,
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
        emails: BTreeMap<String, EmailImport>,
    ) -> Result<JmapEmailImportOutput, JmapClientStdError> {
        let coroutine = JmapEmailImport::new(self.session_or_err()?, &self.http_auth, emails)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailParse`] (`Email/parse`).
    pub fn email_parse(
        &mut self,
        blob_ids: Vec<String>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<JmapEmailParseOutput, JmapClientStdError> {
        let coroutine = JmapEmailParse::new(
            self.session_or_err()?,
            &self.http_auth,
            blob_ids,
            properties,
        )?;
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
        max_changes: Option<u64>,
    ) -> Result<crate::rfc8620::changes::JmapChangesOutput, JmapClientStdError> {
        let coroutine = JmapThreadChanges::new(
            self.session_or_err()?,
            &self.http_auth,
            since_state,
            max_changes,
        )?;
        self.run(coroutine)
    }

    // ---- Identity (RFC 8621 §6) ------------------------------------------

    /// Runs [`JmapIdentityGet`] (`Identity/get`). Pass `ids: None` to
    /// fetch all identities.
    pub fn identity_get(
        &mut self,
        ids: Option<Vec<String>>,
    ) -> Result<JmapIdentityGetOutput, JmapClientStdError> {
        let coroutine = JmapIdentityGet::new(self.session_or_err()?, &self.http_auth, ids)?;
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
        ids: Option<Vec<String>>,
    ) -> Result<JmapEmailSubmissionGetOutput, JmapClientStdError> {
        let coroutine = JmapEmailSubmissionGet::new(self.session_or_err()?, &self.http_auth, ids)?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailSubmissionQuery`] (batched
    /// `EmailSubmission/query` + `EmailSubmission/get`).
    pub fn email_submission_query(
        &mut self,
        filter: Option<EmailSubmissionFilter>,
        sort: Option<Vec<EmailSubmissionComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
    ) -> Result<JmapEmailSubmissionQueryOutput, JmapClientStdError> {
        let coroutine = JmapEmailSubmissionQuery::new(
            self.session_or_err()?,
            &self.http_auth,
            filter,
            sort,
            position,
            limit,
        )?;
        self.run(coroutine)
    }

    /// Runs [`JmapEmailSubmissionSet`] (`EmailSubmission/set`).
    pub fn email_submission_set(
        &mut self,
        submissions: BTreeMap<String, EmailSubmissionCreate>,
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

    /// Runs [`JmapVacationResponseGet`] (`VacationResponse/get`).
    /// Returns the singleton vacation response, if any.
    pub fn vacation_response_get(
        &mut self,
    ) -> Result<Option<VacationResponse>, JmapClientStdError> {
        let coroutine = JmapVacationResponseGet::new(self.session_or_err()?, &self.http_auth)?;
        Ok(self.run(coroutine)?.vacation_response)
    }

    /// Runs [`JmapVacationResponseSet`] (`VacationResponse/set`).
    /// Returns the updated singleton, if the server echoed it back.
    pub fn vacation_response_set(
        &mut self,
        patch: VacationResponseUpdate,
    ) -> Result<Option<VacationResponse>, JmapClientStdError> {
        let coroutine =
            JmapVacationResponseSet::new(self.session_or_err()?, &self.http_auth, patch)?;
        Ok(self.run(coroutine)?.updated)
    }
}
