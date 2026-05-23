//! # Standard, blocking JMAP client
//!
//! Holds a single boxed [`Stream`] (any blocking `Read + Write` impl)
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

use crate::{
    rfc8620::{
        blob_download::*, blob_upload::*, changes::JmapChangesOutput, send::*,
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

    #[error("JMAP server redirected during a non-redirectable operation")]
    UnexpectedRedirect,
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
trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send + ?Sized> Stream for T {}

/// Std-blocking JMAP client wrapping a single [`Stream`].
pub struct JmapClientStd {
    stream: Box<dyn Stream>,
    http_auth: SecretString,
    session: Option<JmapSession>,
}

impl fmt::Debug for JmapClientStd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JmapClientStd")
            .field("http_auth", &self.http_auth)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

/// Run a coroutine against `$self.stream`. The `Ok` pattern
/// destructures whichever fields the caller wants and `$ret` is the
/// value returned on success. Pass the coroutine constructor
/// expression (including the trailing `?`) directly: the macro takes
/// ownership of the resulting coroutine.
macro_rules! coroutine {
    ($self:ident, $coroutine:expr, $Result:ident, $ok_pat:tt => $ret:expr) => {{
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut arg: Option<&[u8]> = None;
        let mut coroutine = $coroutine;

        loop {
            match coroutine.resume(arg) {
                $Result::Ok $ok_pat => return Ok($ret),
                $Result::WantsRead => {
                    let n = $self.stream.read(&mut buf)?;
                    arg = Some(&buf[..n]);
                }
                $Result::WantsWrite(bytes) => {
                    $self.stream.write_all(&bytes)?;
                    arg = None;
                }
                $Result::Err(err) => return Err(err.into()),
            }
        }
    }};
}

impl JmapClientStd {
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
            match coroutine.resume(arg) {
                JmapSessionGetResult::Ok { session, .. } => {
                    self.session = Some(session);
                    return Ok(self.session.as_ref().unwrap());
                }
                JmapSessionGetResult::WantsRead => {
                    let n = self.stream.read(&mut buf)?;
                    arg = Some(&buf[..n]);
                }
                JmapSessionGetResult::WantsWrite(bytes) => {
                    self.stream.write_all(&bytes)?;
                    arg = None;
                }
                JmapSessionGetResult::WantsRedirect { .. } => {
                    return Err(JmapClientStdError::UnexpectedRedirect);
                }
                JmapSessionGetResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Sends a raw JMAP request and returns the raw [`JmapResponse`].
    /// Lower level than the per-method helpers: useful for passthrough
    /// CLIs and ad-hoc requests with custom `using` capabilities.
    pub fn send_raw(&mut self, request: JmapRequest) -> Result<JmapResponse, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapSend::new(&self.http_auth, &session.api_url, request)?,
            JmapSendResult,
            { response, .. } => response
        );
    }

    // ---- Blob (RFC 8620 §6) ----------------------------------------------

    /// Uploads a blob to `upload_url` (RFC 8620 §6.1). The caller must
    /// resolve the session's `uploadUrl` template (e.g. substitute
    /// `{accountId}`) before passing it here.
    pub fn blob_upload(
        &mut self,
        upload_url: &Url,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<JmapBlobUploadOutput, JmapClientStdError> {
        coroutine!(
            self,
            JmapBlobUpload::new(&self.http_auth, upload_url, content_type, data),
            JmapBlobUploadResult,
            { blob_id, blob_type, size, .. } => JmapBlobUploadOutput { blob_id, blob_type, size }
        );
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
            match coroutine.resume(arg) {
                JmapBlobDownloadResult::Ok { data, .. } => return Ok(data),
                JmapBlobDownloadResult::WantsRead => {
                    let n = self.stream.read(&mut buf)?;
                    arg = Some(&buf[..n]);
                }
                JmapBlobDownloadResult::WantsWrite(bytes) => {
                    self.stream.write_all(&bytes)?;
                    arg = None;
                }
                JmapBlobDownloadResult::WantsRedirect { .. } => {
                    return Err(JmapClientStdError::UnexpectedRedirect);
                }
                JmapBlobDownloadResult::Err(err) => return Err(err.into()),
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
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapMailboxGet::new(session, &self.http_auth, ids, properties)?,
            JmapMailboxGetResult,
            { mailboxes, not_found, new_state, .. } =>
                JmapMailboxGetOutput { mailboxes, not_found, new_state }
        );
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
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapMailboxQuery::new(
                session,
                &self.http_auth,
                filter,
                sort,
                position,
                limit,
                properties,
            )?,
            JmapMailboxQueryResult,
            { mailboxes, total, position, query_state, .. } =>
                JmapMailboxQueryOutput { mailboxes, total, position, query_state }
        );
    }

    /// Runs [`JmapMailboxSet`] (`Mailbox/set`).
    pub fn mailbox_set(
        &mut self,
        args: JmapMailboxSetArgs,
    ) -> Result<JmapMailboxSetOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapMailboxSet::new(session, &self.http_auth, args)?,
            JmapMailboxSetResult,
            { new_state, created, updated, destroyed, not_created, not_updated, not_destroyed, .. } =>
                JmapMailboxSetOutput {
                    new_state, created, updated, destroyed,
                    not_created, not_updated, not_destroyed,
                }
        );
    }

    /// Runs [`JmapMailboxChanges`] (`Mailbox/changes`).
    pub fn mailbox_changes(
        &mut self,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<JmapChangesOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapMailboxChanges::new(session, &self.http_auth, since_state, max_changes)?,
            JmapMailboxChangesResult,
            { new_state, has_more_changes, created, updated, destroyed, .. } =>
                JmapChangesOutput { new_state, has_more_changes, created, updated, destroyed }
        );
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
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailGet::new(
                session,
                &self.http_auth,
                ids,
                properties,
                fetch_text_body_values,
                fetch_html_body_values,
                max_body_value_bytes,
            )?,
            JmapEmailGetResult,
            { emails, not_found, new_state, .. } =>
                JmapEmailGetOutput { emails, not_found, new_state }
        );
    }

    /// Runs [`JmapEmailQuery`] (batched `Email/query` + `Email/get`).
    pub fn email_query(
        &mut self,
        filter: Option<EmailFilter>,
        sort: Option<Vec<EmailComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<JmapEmailQueryOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailQuery::new(
                session,
                &self.http_auth,
                filter,
                sort,
                position,
                limit,
                properties,
            )?,
            JmapEmailQueryResult,
            { emails, total, position, query_state, .. } =>
                JmapEmailQueryOutput { emails, total, position, query_state }
        );
    }

    /// Runs [`JmapEmailSet`] (`Email/set`).
    pub fn email_set(
        &mut self,
        args: JmapEmailSetArgs,
    ) -> Result<JmapEmailSetOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailSet::new(session, &self.http_auth, args)?,
            JmapEmailSetResult,
            { new_state, created, updated, destroyed, not_created, not_updated, not_destroyed, .. } =>
                JmapEmailSetOutput {
                    new_state, created, updated, destroyed,
                    not_created, not_updated, not_destroyed,
                }
        );
    }

    /// Runs [`JmapEmailChanges`] (`Email/changes`).
    pub fn email_changes(
        &mut self,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<JmapChangesOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailChanges::new(session, &self.http_auth, since_state, max_changes)?,
            JmapEmailChangesResult,
            { new_state, has_more_changes, created, updated, destroyed, .. } =>
                JmapChangesOutput { new_state, has_more_changes, created, updated, destroyed }
        );
    }

    /// Runs [`JmapEmailCopy`] (`Email/copy`).
    pub fn email_copy(
        &mut self,
        from_account_id: impl Into<String>,
        emails: BTreeMap<String, EmailCopy>,
    ) -> Result<JmapEmailCopyOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailCopy::new(session, &self.http_auth, from_account_id, emails)?,
            JmapEmailCopyResult,
            { new_state, created, not_created, .. } =>
                JmapEmailCopyOutput { new_state, created, not_created }
        );
    }

    /// Runs [`JmapEmailImport`] (`Email/import`).
    pub fn email_import(
        &mut self,
        emails: BTreeMap<String, EmailImport>,
    ) -> Result<JmapEmailImportOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailImport::new(session, &self.http_auth, emails)?,
            JmapEmailImportResult,
            { new_state, created, not_created, .. } =>
                JmapEmailImportOutput { new_state, created, not_created }
        );
    }

    /// Runs [`JmapEmailParse`] (`Email/parse`).
    pub fn email_parse(
        &mut self,
        blob_ids: Vec<String>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<JmapEmailParseOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailParse::new(session, &self.http_auth, blob_ids, properties)?,
            JmapEmailParseResult,
            { parsed, not_parsable, not_found, .. } =>
                JmapEmailParseOutput { parsed, not_parsable, not_found }
        );
    }

    // ---- Thread (RFC 8621 §3) --------------------------------------------

    /// Runs [`JmapThreadGet`] (`Thread/get`).
    pub fn thread_get(
        &mut self,
        ids: Vec<String>,
    ) -> Result<JmapThreadGetOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapThreadGet::new(session, &self.http_auth, ids)?,
            JmapThreadGetResult,
            { threads, not_found, new_state, .. } =>
                JmapThreadGetOutput { threads, not_found, new_state }
        );
    }

    /// Runs [`JmapThreadChanges`] (`Thread/changes`).
    pub fn thread_changes(
        &mut self,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<JmapChangesOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapThreadChanges::new(session, &self.http_auth, since_state, max_changes)?,
            JmapThreadChangesResult,
            { new_state, has_more_changes, created, updated, destroyed, .. } =>
                JmapChangesOutput { new_state, has_more_changes, created, updated, destroyed }
        );
    }

    // ---- Identity (RFC 8621 §6) ------------------------------------------

    /// Runs [`JmapIdentityGet`] (`Identity/get`). Pass `ids: None` to
    /// fetch all identities.
    pub fn identity_get(
        &mut self,
        ids: Option<Vec<String>>,
    ) -> Result<JmapIdentityGetOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapIdentityGet::new(session, &self.http_auth, ids)?,
            JmapIdentityGetResult,
            { identities, not_found, new_state, .. } =>
                JmapIdentityGetOutput { identities, not_found, new_state }
        );
    }

    /// Runs [`JmapIdentitySet`] (`Identity/set`).
    pub fn identity_set(
        &mut self,
        args: JmapIdentitySetArgs,
    ) -> Result<JmapIdentitySetOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapIdentitySet::new(session, &self.http_auth, args)?,
            JmapIdentitySetResult,
            { new_state, created, updated, destroyed, not_created, not_updated, not_destroyed, .. } =>
                JmapIdentitySetOutput {
                    new_state, created, updated, destroyed,
                    not_created, not_updated, not_destroyed,
                }
        );
    }

    // ---- EmailSubmission (RFC 8621 §7) -----------------------------------

    /// Runs [`JmapEmailSubmissionGet`] (`EmailSubmission/get`).
    pub fn email_submission_get(
        &mut self,
        ids: Option<Vec<String>>,
    ) -> Result<JmapEmailSubmissionGetOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailSubmissionGet::new(session, &self.http_auth, ids)?,
            JmapEmailSubmissionGetResult,
            { submissions, not_found, new_state, .. } =>
                JmapEmailSubmissionGetOutput { submissions, not_found, new_state }
        );
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
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailSubmissionQuery::new(session, &self.http_auth, filter, sort, position, limit)?,
            JmapEmailSubmissionQueryResult,
            { submissions, total, position, query_state, .. } =>
                JmapEmailSubmissionQueryOutput { submissions, total, position, query_state }
        );
    }

    /// Runs [`JmapEmailSubmissionSet`] (`EmailSubmission/set`).
    pub fn email_submission_set(
        &mut self,
        submissions: BTreeMap<String, EmailSubmissionCreate>,
    ) -> Result<JmapEmailSubmissionSetOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailSubmissionSet::new(session, &self.http_auth, submissions)?,
            JmapEmailSubmissionSetResult,
            { new_state, created, not_created, .. } =>
                JmapEmailSubmissionSetOutput { new_state, created, not_created }
        );
    }

    /// Runs [`JmapEmailSubmissionCancel`] (`EmailSubmission/set` with
    /// `undoStatus: "canceled"`).
    pub fn email_submission_cancel(
        &mut self,
        ids: Vec<String>,
    ) -> Result<JmapEmailSubmissionCancelOutput, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapEmailSubmissionCancel::new(session, &self.http_auth, ids)?,
            JmapEmailSubmissionCancelResult,
            { new_state, updated, not_updated, .. } =>
                JmapEmailSubmissionCancelOutput { new_state, updated, not_updated }
        );
    }

    // ---- VacationResponse (RFC 8621 §8) ----------------------------------

    /// Runs [`JmapVacationResponseGet`] (`VacationResponse/get`).
    /// Returns the singleton vacation response, if any.
    pub fn vacation_response_get(
        &mut self,
    ) -> Result<Option<VacationResponse>, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapVacationResponseGet::new(session, &self.http_auth)?,
            JmapVacationResponseGetResult,
            { vacation_response, .. } => vacation_response
        );
    }

    /// Runs [`JmapVacationResponseSet`] (`VacationResponse/set`).
    /// Returns the updated singleton, if the server echoed it back.
    pub fn vacation_response_set(
        &mut self,
        patch: VacationResponseUpdate,
    ) -> Result<Option<VacationResponse>, JmapClientStdError> {
        let session = self.session_or_err()?;
        coroutine!(
            self,
            JmapVacationResponseSet::new(session, &self.http_auth, patch)?,
            JmapVacationResponseSetResult,
            { updated, .. } => updated
        );
    }
}
