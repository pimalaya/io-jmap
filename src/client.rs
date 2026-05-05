//! # Standard, blocking JMAP client
//!
//! Holds a single boxed [`JmapStream`] (any blocking `Read + Write`
//! impl) plus the bearer token and discovered [`JmapSession`], and
//! exposes one method per common coroutine. The bare [`new`]
//! constructor takes a pre-connected stream — callers handle TCP and
//! TLS themselves. With one of the TLS feature flags enabled
//! (`rustls-ring`, `rustls-aws`, `native-tls`), [`connect`] is also
//! available and handles `https://` URLs end-to-end via
//! [`pimalaya_stream::tls::upgrade_tls`].
//!
//! After construction, the caller must drive [`session_get`] once to
//! discover the JMAP session object (RFC 8620 §2). All subsequent
//! method calls use that cached session for `accountId` resolution
//! and the `apiUrl` endpoint.
//!
//! [`new`]: JmapClient::new
//! [`connect`]: JmapClient::connect
//! [`session_get`]: JmapClient::session_get

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
use alloc::string::ToString;
use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
use std::io::{Read, Write};

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
use std::net::TcpStream;

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
use pimalaya_stream::tls::{Tls, upgrade_tls};
use secrecy::SecretString;
use thiserror::Error;
use url::Url;

use crate::{
    rfc8620::{blob_download::*, blob_upload::*, session::JmapSession, session_get::*},
    rfc8621::{
        email::{
            Email, EmailComparator, EmailCopy, EmailCopyError, EmailFilter, EmailImport,
            EmailImportError, EmailProperty, EmailSetError,
        },
        email_changes::*,
        email_copy::*,
        email_get::*,
        email_import::*,
        email_parse::*,
        email_query::*,
        email_set::*,
        email_submission::{
            EmailSubmission, EmailSubmissionComparator, EmailSubmissionCreate,
            EmailSubmissionFilter, EmailSubmissionSetError,
        },
        email_submission_cancel::*,
        email_submission_get::*,
        email_submission_query::*,
        email_submission_set::*,
        identity::{Identity, IdentitySetError},
        identity_get::*,
        identity_set::*,
        mailbox::{
            Mailbox, MailboxFilter, MailboxProperty, MailboxSetError, MailboxSortComparator,
        },
        mailbox_changes::*,
        mailbox_get::*,
        mailbox_query::*,
        mailbox_set::*,
        thread::Thread,
        thread_changes::*,
        thread_get::*,
        vacation_response::{VacationResponse, VacationResponseUpdate},
        vacation_response_get::*,
        vacation_response_set::*,
    },
};

const READ_BUFFER_SIZE: usize = 16 * 1024;

/// Open marker for everything the client can drive — auto-implemented
/// for any blocking `Read + Write`.
pub trait JmapStream: Read + Write {}
impl<T: Read + Write + ?Sized> JmapStream for T {}

/// Errors returned by [`JmapClient`].
#[derive(Debug, Error)]
pub enum JmapClientError {
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
    Io(#[from] std::io::Error),

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
    #[error("JMAP client missing session — call `session_get` first")]
    MissingSession,
}

/// Output of [`JmapClient::blob_upload`].
#[derive(Clone, Debug)]
pub struct JmapBlobUploadOutput {
    pub blob_id: String,
    pub blob_type: String,
    pub size: u64,
}

/// Output of `Foo/changes` calls
/// ([`JmapClient::mailbox_changes`], [`JmapClient::email_changes`],
/// [`JmapClient::thread_changes`]).
#[derive(Clone, Debug)]
pub struct JmapChangesOutput {
    pub new_state: String,
    pub has_more_changes: bool,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub destroyed: Vec<String>,
}

/// Output of [`JmapClient::mailbox_get`].
#[derive(Clone, Debug)]
pub struct JmapMailboxGetOutput {
    pub mailboxes: Vec<Mailbox>,
    pub not_found: Vec<String>,
    pub new_state: String,
}

/// Output of [`JmapClient::mailbox_query`].
#[derive(Clone, Debug)]
pub struct JmapMailboxQueryOutput {
    pub mailboxes: Vec<Mailbox>,
    pub total: Option<u64>,
    pub position: u64,
    pub query_state: String,
}

/// Output of [`JmapClient::mailbox_set`].
#[derive(Clone, Debug)]
pub struct JmapMailboxSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Mailbox>,
    pub updated: BTreeMap<String, Option<Mailbox>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, MailboxSetError>,
    pub not_updated: BTreeMap<String, MailboxSetError>,
    pub not_destroyed: BTreeMap<String, MailboxSetError>,
}

/// Output of [`JmapClient::email_get`].
#[derive(Clone, Debug)]
pub struct JmapEmailGetOutput {
    pub emails: Vec<Email>,
    pub not_found: Vec<String>,
    pub new_state: String,
}

/// Output of [`JmapClient::email_query`].
#[derive(Clone, Debug)]
pub struct JmapEmailQueryOutput {
    pub emails: Vec<Email>,
    pub total: Option<u64>,
    pub position: u64,
    pub query_state: String,
}

/// Output of [`JmapClient::email_set`].
#[derive(Clone, Debug)]
pub struct JmapEmailSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Email>,
    pub updated: BTreeMap<String, Option<Email>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, EmailSetError>,
    pub not_updated: BTreeMap<String, EmailSetError>,
    pub not_destroyed: BTreeMap<String, EmailSetError>,
}

/// Output of [`JmapClient::email_copy`].
#[derive(Clone, Debug)]
pub struct JmapEmailCopyOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Email>,
    pub not_created: BTreeMap<String, EmailCopyError>,
}

/// Output of [`JmapClient::email_import`].
#[derive(Clone, Debug)]
pub struct JmapEmailImportOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Email>,
    pub not_created: BTreeMap<String, EmailImportError>,
}

/// Output of [`JmapClient::email_parse`].
#[derive(Clone, Debug)]
pub struct JmapEmailParseOutput {
    pub parsed: BTreeMap<String, Email>,
    pub not_parsable: Vec<String>,
    pub not_found: Vec<String>,
}

/// Output of [`JmapClient::thread_get`].
#[derive(Clone, Debug)]
pub struct JmapThreadGetOutput {
    pub threads: Vec<Thread>,
    pub not_found: Vec<String>,
    pub new_state: String,
}

/// Output of [`JmapClient::identity_get`].
#[derive(Clone, Debug)]
pub struct JmapIdentityGetOutput {
    pub identities: Vec<Identity>,
    pub not_found: Vec<String>,
    pub new_state: String,
}

/// Output of [`JmapClient::identity_set`].
#[derive(Clone, Debug)]
pub struct JmapIdentitySetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, Identity>,
    pub updated: BTreeMap<String, Option<Identity>>,
    pub destroyed: Vec<String>,
    pub not_created: BTreeMap<String, IdentitySetError>,
    pub not_updated: BTreeMap<String, IdentitySetError>,
    pub not_destroyed: BTreeMap<String, IdentitySetError>,
}

/// Output of [`JmapClient::email_submission_get`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionGetOutput {
    pub submissions: Vec<EmailSubmission>,
    pub not_found: Vec<String>,
    pub new_state: String,
}

/// Output of [`JmapClient::email_submission_query`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionQueryOutput {
    pub submissions: Vec<EmailSubmission>,
    pub total: Option<u64>,
    pub position: u64,
    pub query_state: String,
}

/// Output of [`JmapClient::email_submission_set`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionSetOutput {
    pub new_state: String,
    pub created: BTreeMap<String, EmailSubmission>,
    pub not_created: BTreeMap<String, EmailSubmissionSetError>,
}

/// Output of [`JmapClient::email_submission_cancel`].
#[derive(Clone, Debug)]
pub struct JmapEmailSubmissionCancelOutput {
    pub new_state: String,
    pub updated: BTreeMap<String, Option<EmailSubmission>>,
    pub not_updated: BTreeMap<String, EmailSubmissionSetError>,
}

/// Std-blocking JMAP client wrapping a single [`JmapStream`].
pub struct JmapClient {
    stream: Box<dyn JmapStream>,
    http_auth: SecretString,
    session: Option<JmapSession>,
}

/// Drive a coroutine whose result variants follow the standard
/// `Ok { .. } / WantsRead / WantsWrite(Vec<u8>) / Err(_)` shape.
/// `$ok_pat` destructures the `Ok` fields and `$on_ok` returns the
/// method's output value.
macro_rules! drive {
    ($self:ident, $coroutine:expr, $Result:ident, $ok_pat:tt => $on_ok:expr) => {{
        let mut coroutine = $coroutine;
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut arg: Option<&[u8]> = None;
        loop {
            match coroutine.resume(arg) {
                $Result::Ok $ok_pat => return Ok($on_ok),
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

impl JmapClient {
    /// Builds a client around `stream`. The caller is responsible
    /// for opening the connection (TCP, TLS handshake if needed) and
    /// for the bearer token / authorization header value.
    pub fn new<S: Read + Write + 'static>(stream: S, http_auth: SecretString) -> Self {
        Self {
            stream: Box::new(stream),
            http_auth,
            session: None,
        }
    }

    /// Connects to `url` and runs the TLS handshake when the scheme
    /// is `https` or `jmaps`. `http` and `jmap` go through plain TCP.
    /// ALPN is set to `http/1.1`.
    #[cfg(any(
        feature = "rustls-aws",
        feature = "rustls-ring",
        feature = "native-tls"
    ))]
    pub fn connect(url: &Url, tls: &Tls, http_auth: SecretString) -> Result<Self, JmapClientError> {
        let host = url
            .host_str()
            .ok_or_else(|| JmapClientError::UrlMissingHost(url.to_string()))?;

        let stream: Box<dyn JmapStream> = match url.scheme() {
            scheme
                if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("jmap") =>
            {
                let port = url.port().unwrap_or(80);
                Box::new(TcpStream::connect((host, port))?)
            }
            scheme
                if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("jmaps") =>
            {
                let port = url.port().unwrap_or(443);
                let tcp = TcpStream::connect((host, port))?;
                Box::new(upgrade_tls(host, tcp, tls, &[b"http/1.1"])?)
            }
            scheme => {
                return Err(JmapClientError::UrlUnsupportedScheme(
                    url.to_string(),
                    scheme.to_string(),
                ));
            }
        };

        Ok(Self {
            stream,
            http_auth,
            session: None,
        })
    }

    /// Replaces the underlying stream — useful when JMAP redirects to
    /// a different host or when the session's `apiUrl`, `uploadUrl`,
    /// or `downloadUrl` lives on a different authority than where the
    /// client first connected.
    pub fn set_stream<S: Read + Write + 'static>(&mut self, stream: S) {
        self.stream = Box::new(stream);
    }

    /// Returns the cached session, if [`session_get`] has run.
    ///
    /// [`session_get`]: JmapClient::session_get
    pub fn session(&self) -> Option<&JmapSession> {
        self.session.as_ref()
    }

    fn session_or_err(&self) -> Result<&JmapSession, JmapClientError> {
        self.session.as_ref().ok_or(JmapClientError::MissingSession)
    }

    /// Drives [`JmapSessionGet`] and caches the discovered session.
    ///
    /// Pass either a base URL for `/.well-known/jmap` discovery
    /// (`https://mail.example.com`) or a direct session endpoint
    /// (`https://api.example.com/jmap/session/`).
    ///
    /// A redirect terminates the call with
    /// [`JmapClientError::UnexpectedRedirect`] — the caller must open
    /// a new connection to the redirect target and retry.
    pub fn session_get(&mut self, url: &Url) -> Result<&JmapSession, JmapClientError> {
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
                    return Err(JmapClientError::UnexpectedRedirect);
                }
                JmapSessionGetResult::Err(err) => return Err(err.into()),
            }
        }
    }

    // ---- Blob (RFC 8620 §6) ----------------------------------------------

    /// Uploads a blob to `upload_url` (RFC 8620 §6.1). The caller
    /// must resolve the session's `uploadUrl` template (e.g.
    /// substitute `{accountId}`) before passing it here.
    pub fn blob_upload(
        &mut self,
        upload_url: &Url,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<JmapBlobUploadOutput, JmapClientError> {
        let coroutine = JmapBlobUpload::new(&self.http_auth, upload_url, content_type, data);
        drive!(
            self,
            coroutine,
            JmapBlobUploadResult,
            { blob_id, blob_type, size, .. } => JmapBlobUploadOutput { blob_id, blob_type, size }
        );
    }

    /// Downloads a blob from `download_url` (RFC 8620 §6.2). The
    /// caller must resolve the session's `downloadUrl` template
    /// before passing it here.
    ///
    /// A redirect terminates the call with
    /// [`JmapClientError::UnexpectedRedirect`].
    pub fn blob_download(&mut self, download_url: &Url) -> Result<Vec<u8>, JmapClientError> {
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
                    return Err(JmapClientError::UnexpectedRedirect);
                }
                JmapBlobDownloadResult::Err(err) => return Err(err.into()),
            }
        }
    }

    // ---- Mailbox (RFC 8621 §2) -------------------------------------------

    /// Drives [`JmapMailboxGet`] (`Mailbox/get`).
    pub fn mailbox_get(
        &mut self,
        ids: Option<Vec<String>>,
        properties: Option<Vec<MailboxProperty>>,
    ) -> Result<JmapMailboxGetOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapMailboxGet::new(session, &self.http_auth, ids, properties)?;
        drive!(
            self,
            coroutine,
            JmapMailboxGetResult,
            { mailboxes, not_found, new_state, .. } => JmapMailboxGetOutput { mailboxes, not_found, new_state }
        );
    }

    /// Drives [`JmapMailboxQuery`] (batched `Mailbox/query` +
    /// `Mailbox/get`).
    pub fn mailbox_query(
        &mut self,
        filter: Option<MailboxFilter>,
        sort: Option<Vec<MailboxSortComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
        properties: Option<Vec<MailboxProperty>>,
    ) -> Result<JmapMailboxQueryOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapMailboxQuery::new(
            session,
            &self.http_auth,
            filter,
            sort,
            position,
            limit,
            properties,
        )?;
        drive!(
            self,
            coroutine,
            JmapMailboxQueryResult,
            { mailboxes, total, position, query_state, .. } =>
                JmapMailboxQueryOutput { mailboxes, total, position, query_state }
        );
    }

    /// Drives [`JmapMailboxSet`] (`Mailbox/set`).
    pub fn mailbox_set(
        &mut self,
        args: JmapMailboxSetArgs,
    ) -> Result<JmapMailboxSetOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapMailboxSet::new(session, &self.http_auth, args)?;
        drive!(
            self,
            coroutine,
            JmapMailboxSetResult,
            { new_state, created, updated, destroyed, not_created, not_updated, not_destroyed, .. } =>
                JmapMailboxSetOutput {
                    new_state, created, updated, destroyed,
                    not_created, not_updated, not_destroyed,
                }
        );
    }

    /// Drives [`JmapMailboxChanges`] (`Mailbox/changes`).
    pub fn mailbox_changes(
        &mut self,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<JmapChangesOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine =
            JmapMailboxChanges::new(session, &self.http_auth, since_state, max_changes)?;
        drive!(
            self,
            coroutine,
            JmapMailboxChangesResult,
            { new_state, has_more_changes, created, updated, destroyed, .. } =>
                JmapChangesOutput { new_state, has_more_changes, created, updated, destroyed }
        );
    }

    // ---- Email (RFC 8621 §4) ---------------------------------------------

    /// Drives [`JmapEmailGet`] (`Email/get`).
    pub fn email_get(
        &mut self,
        ids: Vec<String>,
        properties: Option<Vec<String>>,
        fetch_text_body_values: bool,
        fetch_html_body_values: bool,
        max_body_value_bytes: u64,
    ) -> Result<JmapEmailGetOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailGet::new(
            session,
            &self.http_auth,
            ids,
            properties,
            fetch_text_body_values,
            fetch_html_body_values,
            max_body_value_bytes,
        )?;
        drive!(
            self,
            coroutine,
            JmapEmailGetResult,
            { emails, not_found, new_state, .. } => JmapEmailGetOutput { emails, not_found, new_state }
        );
    }

    /// Drives [`JmapEmailQuery`] (batched `Email/query` + `Email/get`).
    pub fn email_query(
        &mut self,
        filter: Option<EmailFilter>,
        sort: Option<Vec<EmailComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<JmapEmailQueryOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailQuery::new(
            session,
            &self.http_auth,
            filter,
            sort,
            position,
            limit,
            properties,
        )?;
        drive!(
            self,
            coroutine,
            JmapEmailQueryResult,
            { emails, total, position, query_state, .. } =>
                JmapEmailQueryOutput { emails, total, position, query_state }
        );
    }

    /// Drives [`JmapEmailSet`] (`Email/set`).
    pub fn email_set(
        &mut self,
        args: JmapEmailSetArgs,
    ) -> Result<JmapEmailSetOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailSet::new(session, &self.http_auth, args)?;
        drive!(
            self,
            coroutine,
            JmapEmailSetResult,
            { new_state, created, updated, destroyed, not_created, not_updated, not_destroyed, .. } =>
                JmapEmailSetOutput {
                    new_state, created, updated, destroyed,
                    not_created, not_updated, not_destroyed,
                }
        );
    }

    /// Drives [`JmapEmailChanges`] (`Email/changes`).
    pub fn email_changes(
        &mut self,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<JmapChangesOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailChanges::new(session, &self.http_auth, since_state, max_changes)?;
        drive!(
            self,
            coroutine,
            JmapEmailChangesResult,
            { new_state, has_more_changes, created, updated, destroyed, .. } =>
                JmapChangesOutput { new_state, has_more_changes, created, updated, destroyed }
        );
    }

    /// Drives [`JmapEmailCopy`] (`Email/copy`).
    pub fn email_copy(
        &mut self,
        from_account_id: impl Into<String>,
        emails: BTreeMap<String, EmailCopy>,
    ) -> Result<JmapEmailCopyOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailCopy::new(session, &self.http_auth, from_account_id, emails)?;
        drive!(
            self,
            coroutine,
            JmapEmailCopyResult,
            { new_state, created, not_created, .. } =>
                JmapEmailCopyOutput { new_state, created, not_created }
        );
    }

    /// Drives [`JmapEmailImport`] (`Email/import`).
    pub fn email_import(
        &mut self,
        emails: BTreeMap<String, EmailImport>,
    ) -> Result<JmapEmailImportOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailImport::new(session, &self.http_auth, emails)?;
        drive!(
            self,
            coroutine,
            JmapEmailImportResult,
            { new_state, created, not_created, .. } =>
                JmapEmailImportOutput { new_state, created, not_created }
        );
    }

    /// Drives [`JmapEmailParse`] (`Email/parse`).
    pub fn email_parse(
        &mut self,
        blob_ids: Vec<String>,
        properties: Option<Vec<EmailProperty>>,
    ) -> Result<JmapEmailParseOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailParse::new(session, &self.http_auth, blob_ids, properties)?;
        drive!(
            self,
            coroutine,
            JmapEmailParseResult,
            { parsed, not_parsable, not_found, .. } =>
                JmapEmailParseOutput { parsed, not_parsable, not_found }
        );
    }

    // ---- Thread (RFC 8621 §3) --------------------------------------------

    /// Drives [`JmapThreadGet`] (`Thread/get`).
    pub fn thread_get(&mut self, ids: Vec<String>) -> Result<JmapThreadGetOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapThreadGet::new(session, &self.http_auth, ids)?;
        drive!(
            self,
            coroutine,
            JmapThreadGetResult,
            { threads, not_found, new_state, .. } =>
                JmapThreadGetOutput { threads, not_found, new_state }
        );
    }

    /// Drives [`JmapThreadChanges`] (`Thread/changes`).
    pub fn thread_changes(
        &mut self,
        since_state: impl Into<String>,
        max_changes: Option<u64>,
    ) -> Result<JmapChangesOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapThreadChanges::new(session, &self.http_auth, since_state, max_changes)?;
        drive!(
            self,
            coroutine,
            JmapThreadChangesResult,
            { new_state, has_more_changes, created, updated, destroyed, .. } =>
                JmapChangesOutput { new_state, has_more_changes, created, updated, destroyed }
        );
    }

    // ---- Identity (RFC 8621 §6) ------------------------------------------

    /// Drives [`JmapIdentityGet`] (`Identity/get`). Pass `ids: None`
    /// to fetch all identities.
    pub fn identity_get(
        &mut self,
        ids: Option<Vec<String>>,
    ) -> Result<JmapIdentityGetOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapIdentityGet::new(session, &self.http_auth, ids)?;
        drive!(
            self,
            coroutine,
            JmapIdentityGetResult,
            { identities, not_found, new_state, .. } =>
                JmapIdentityGetOutput { identities, not_found, new_state }
        );
    }

    /// Drives [`JmapIdentitySet`] (`Identity/set`).
    pub fn identity_set(
        &mut self,
        args: JmapIdentitySetArgs,
    ) -> Result<JmapIdentitySetOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapIdentitySet::new(session, &self.http_auth, args)?;
        drive!(
            self,
            coroutine,
            JmapIdentitySetResult,
            { new_state, created, updated, destroyed, not_created, not_updated, not_destroyed, .. } =>
                JmapIdentitySetOutput {
                    new_state, created, updated, destroyed,
                    not_created, not_updated, not_destroyed,
                }
        );
    }

    // ---- EmailSubmission (RFC 8621 §7) -----------------------------------

    /// Drives [`JmapEmailSubmissionGet`] (`EmailSubmission/get`).
    pub fn email_submission_get(
        &mut self,
        ids: Option<Vec<String>>,
    ) -> Result<JmapEmailSubmissionGetOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailSubmissionGet::new(session, &self.http_auth, ids)?;
        drive!(
            self,
            coroutine,
            JmapEmailSubmissionGetResult,
            { submissions, not_found, new_state, .. } =>
                JmapEmailSubmissionGetOutput { submissions, not_found, new_state }
        );
    }

    /// Drives [`JmapEmailSubmissionQuery`] (batched
    /// `EmailSubmission/query` + `EmailSubmission/get`).
    pub fn email_submission_query(
        &mut self,
        filter: Option<EmailSubmissionFilter>,
        sort: Option<Vec<EmailSubmissionComparator>>,
        position: Option<u64>,
        limit: Option<u64>,
    ) -> Result<JmapEmailSubmissionQueryOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine =
            JmapEmailSubmissionQuery::new(session, &self.http_auth, filter, sort, position, limit)?;
        drive!(
            self,
            coroutine,
            JmapEmailSubmissionQueryResult,
            { submissions, total, position, query_state, .. } =>
                JmapEmailSubmissionQueryOutput { submissions, total, position, query_state }
        );
    }

    /// Drives [`JmapEmailSubmissionSet`] (`EmailSubmission/set`).
    pub fn email_submission_set(
        &mut self,
        submissions: BTreeMap<String, EmailSubmissionCreate>,
    ) -> Result<JmapEmailSubmissionSetOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailSubmissionSet::new(session, &self.http_auth, submissions)?;
        drive!(
            self,
            coroutine,
            JmapEmailSubmissionSetResult,
            { new_state, created, not_created, .. } =>
                JmapEmailSubmissionSetOutput { new_state, created, not_created }
        );
    }

    /// Drives [`JmapEmailSubmissionCancel`] (`EmailSubmission/set`
    /// with `undoStatus: "canceled"`).
    pub fn email_submission_cancel(
        &mut self,
        ids: Vec<String>,
    ) -> Result<JmapEmailSubmissionCancelOutput, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapEmailSubmissionCancel::new(session, &self.http_auth, ids)?;
        drive!(
            self,
            coroutine,
            JmapEmailSubmissionCancelResult,
            { new_state, updated, not_updated, .. } =>
                JmapEmailSubmissionCancelOutput { new_state, updated, not_updated }
        );
    }

    // ---- VacationResponse (RFC 8621 §8) ----------------------------------

    /// Drives [`JmapVacationResponseGet`] (`VacationResponse/get`).
    /// Returns the singleton vacation response, if any.
    pub fn vacation_response_get(&mut self) -> Result<Option<VacationResponse>, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapVacationResponseGet::new(session, &self.http_auth)?;
        drive!(
            self,
            coroutine,
            JmapVacationResponseGetResult,
            { vacation_response, .. } => vacation_response
        );
    }

    /// Drives [`JmapVacationResponseSet`] (`VacationResponse/set`).
    /// Returns the updated singleton, if the server echoed it back.
    pub fn vacation_response_set(
        &mut self,
        patch: VacationResponseUpdate,
    ) -> Result<Option<VacationResponse>, JmapClientError> {
        let session = self.session_or_err()?;
        let coroutine = JmapVacationResponseSet::new(session, &self.http_auth, patch)?;
        drive!(
            self,
            coroutine,
            JmapVacationResponseSetResult,
            { updated, .. } => updated
        );
    }
}
