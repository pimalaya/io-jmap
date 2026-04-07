//! Shared helpers for provider integration tests.
//!
//! Each test drives the raw coroutine loop against a live JMAP server.
//! Call [`run_jmap`] for plain HTTP (e.g. a local Stalwart instance) or
//! [`run_jmaps`] for HTTPS (e.g. Fastmail).
//!
//! The full test flow exercises:
//!
//! ```text
//! SESSION GET
//!   → MAILBOX QUERY       (baseline — at least one mailbox exists)
//!   → MAILBOX SET create  (create test mailbox)
//!   → MAILBOX GET         (verify creation)
//!   → MAILBOX SET update  (rename)
//!   → MAILBOX GET         (verify rename)
//!   → BLOB UPLOAD         (upload raw RFC 5322 message)
//!   → EMAIL IMPORT        (import blob into test mailbox)
//!   → EMAIL QUERY         (verify exactly one email in mailbox)
//!   → EMAIL GET           (fetch by id)
//!   → EMAIL SET           (add $seen keyword)
//!   → EMAIL SET           (remove $seen keyword)
//!   → THREAD GET          (verify thread references the email)
//!   → EMAIL SET destroy   (cleanup)
//!   → MAILBOX SET destroy (cleanup)
//! ```

use std::{
    collections::BTreeMap,
    io::{Read, Result as IoResult, Write},
    net::TcpStream,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use io_jmap::{
    rfc8620::{
        blob_upload::{JmapBlobUpload, JmapBlobUploadResult},
        session_get::{JmapSessionGet, JmapSessionGetResult},
    },
    rfc8621::{
        email::{EmailFilter, EmailImport},
        email_get::{JmapEmailGet, JmapEmailGetResult},
        email_import::{JmapEmailImport, JmapEmailImportResult},
        email_query::{JmapEmailQuery, JmapEmailQueryResult},
        email_set::{JmapEmailSet, JmapEmailSetArgs, JmapEmailSetResult},
        mailbox::{MailboxCreate, MailboxUpdate},
        mailbox_get::{JmapMailboxGet, JmapMailboxGetResult},
        mailbox_query::{JmapMailboxQuery, JmapMailboxQueryResult},
        mailbox_set::{JmapMailboxSet, JmapMailboxSetArgs, JmapMailboxSetResult},
        thread_get::{JmapThreadGet, JmapThreadGetResult},
    },
};
use io_socket::runtimes::std_stream::handle;
use rustls::{ClientConfig, ClientConnection, StreamOwned, pki_types::ServerName};
use rustls_platform_verifier::ConfigVerifierExt;
use secrecy::SecretString;
use url::Url;

/// A stream that is either a plain TCP connection or a TLS-wrapped one.
///
/// Allows [`run`] to work with both without dynamic dispatch.
enum JmapStream {
    Plain(TcpStream),
    Tls(StreamOwned<ClientConnection, TcpStream>),
}

impl Read for JmapStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(s) => s.read(buf),
        }
    }
}

impl Write for JmapStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        match self {
            Self::Plain(s) => s.write(buf),
            Self::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> IoResult<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(s) => s.flush(),
        }
    }
}

/// A shared end-to-end JMAP test flow over plain HTTP.
///
/// Connects to `host:port` (no TLS) and runs the full protocol
/// sequence.
///
/// The connect closure always uses the provided `host:port`, ignoring
/// the URL argument.  This is intentional: a local server
/// (e.g. Stalwart) may advertise a machine hostname in `apiUrl` that
/// does not resolve from the test runner's perspective.  The URL is
/// still passed into the coroutines for the HTTP `Host` header and
/// request path — TCP routing is separate.
pub fn run_jmap(host: &str, port: u16, http_auth: &str, email: &str) {
    let _ = env_logger::try_init();
    let h = host.to_owned();
    let p = port;
    let session_url = format!("http://{host}:{port}/jmap/session");
    run(
        &|_url| JmapStream::Plain(TcpStream::connect((h.as_str(), p)).expect("TCP connect")),
        &session_url,
        http_auth,
        email,
    )
}

/// A shared end-to-end JMAP test flow over HTTPS (TLS).
///
/// Connects to `host:port` with TLS and runs the full protocol sequence.
pub fn run_jmaps(host: &str, port: u16, http_auth: &str, email: &str) {
    let _ = env_logger::try_init();
    let session_url = format!("https://{host}/jmap/session");
    run(
        &|url| {
            let host = url.host_str().expect("url host").to_owned();
            let port = url.port_or_known_default().expect("url port");
            let server_name = ServerName::try_from(host.clone()).expect("valid server name");
            let config = ClientConfig::with_platform_verifier().expect("TLS config");
            let conn = ClientConnection::new(Arc::new(config), server_name).expect("TLS handshake");
            let tcp = TcpStream::connect((host.as_str(), port)).expect("TCP connect");
            JmapStream::Tls(StreamOwned::new(conn, tcp))
        },
        &session_url,
        http_auth,
        email,
    )
}

fn run(connect: &dyn Fn(&Url) -> JmapStream, session_url: &str, http_auth: &str, email: &str) {
    let token = SecretString::from(http_auth.to_owned());

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let mbox_name = format!("io-jmap-test-{ts}");
    let session_url = Url::parse(session_url).expect("parse session URL");

    // ── SESSION GET ──────────────────────────────────────────────────────────

    let mut stream = connect(&session_url);
    let mut coroutine = JmapSessionGet::new(&token, &session_url);
    let mut arg = None;

    let session = loop {
        match coroutine.resume(arg.take()) {
            JmapSessionGetResult::Ok {
                session,
                keep_alive,
            } => {
                // Reconnect to the API host (may differ from the session host,
                // e.g. api.fastmail.com → jmap.fastmail.com) or if the server
                // closed the connection.
                if !keep_alive || session.api_url.host_str() != session_url.host_str() {
                    stream = connect(&session.api_url);
                }
                break session;
            }
            JmapSessionGetResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
            JmapSessionGetResult::Redirect { url, .. } => panic!("unexpected redirect to {url}"),
            JmapSessionGetResult::Err { err } => panic!("SESSION GET: {err}"),
        }
    };

    let account_id = session.primary_account_id_for("urn:ietf:params:jmap:mail");
    assert!(
        !account_id.is_empty(),
        "primary mail account id should not be empty"
    );

    let api_url = session.api_url.clone();

    // ── MAILBOX QUERY (baseline) ─────────────────────────────────────────────

    {
        let mut coroutine =
            JmapMailboxQuery::new(&session, &token, None, None, None, None, None).unwrap();
        let mut arg = None;

        let mailboxes = loop {
            match coroutine.resume(arg.take()) {
                JmapMailboxQueryResult::Ok {
                    mailboxes,
                    keep_alive,
                    ..
                } => {
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break mailboxes;
                }
                JmapMailboxQueryResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapMailboxQueryResult::Err { err } => panic!("MAILBOX QUERY: {err}"),
            }
        };

        assert!(
            !mailboxes.is_empty(),
            "mailbox query should return at least one mailbox"
        );
    }

    // ── MAILBOX SET create ───────────────────────────────────────────────────

    let mbox_id = {
        let mut create = BTreeMap::new();
        create.insert(
            "new-mbox".to_owned(),
            MailboxCreate {
                name: Some(mbox_name.clone()),
                is_subscribed: Some(true),
                ..Default::default()
            },
        );
        let args = JmapMailboxSetArgs {
            create: Some(create),
            ..Default::default()
        };

        let mut coroutine =
            JmapMailboxSet::new(&session, &token, args).expect("create mailbox set coroutine");
        let mut arg = None;

        let created = loop {
            match coroutine.resume(arg.take()) {
                JmapMailboxSetResult::Ok {
                    created,
                    not_created,
                    keep_alive,
                    ..
                } => {
                    assert!(
                        not_created.is_empty(),
                        "MAILBOX SET create: not_created = {not_created:?}"
                    );
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break created;
                }
                JmapMailboxSetResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapMailboxSetResult::Err { err } => panic!("MAILBOX SET create: {err}"),
            }
        };

        created
            .get("new-mbox")
            .expect("created mailbox missing from MAILBOX SET response")
            .id
            .clone()
            .expect("created mailbox has no id")
    };

    // ── MAILBOX GET (verify creation) ────────────────────────────────────────

    {
        let mut coroutine =
            JmapMailboxGet::new(&session, &token, Some(vec![mbox_id.clone()]), None)
                .expect("create mailbox get coroutine");
        let mut arg = None;

        let mailboxes = loop {
            match coroutine.resume(arg.take()) {
                JmapMailboxGetResult::Ok {
                    mailboxes,
                    not_found,
                    keep_alive,
                    ..
                } => {
                    assert!(
                        not_found.is_empty(),
                        "MAILBOX GET: not_found = {not_found:?}"
                    );
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break mailboxes;
                }
                JmapMailboxGetResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapMailboxGetResult::Err { err } => panic!("MAILBOX GET: {err}"),
            }
        };

        assert_eq!(
            mailboxes[0].id.as_deref(),
            Some(mbox_id.as_str()),
            "MAILBOX GET: id mismatch"
        );
        assert_eq!(
            mailboxes[0].name.as_deref(),
            Some(mbox_name.as_str()),
            "MAILBOX GET: name mismatch"
        );
    }

    // ── MAILBOX SET update (rename) ──────────────────────────────────────────

    let mbox_name_2 = format!("{mbox_name}-renamed");

    {
        let mut update = BTreeMap::new();
        update.insert(
            mbox_id.clone(),
            MailboxUpdate {
                name: Some(mbox_name_2.clone()),
                ..Default::default()
            },
        );
        let args = JmapMailboxSetArgs {
            update: Some(update),
            ..Default::default()
        };

        let mut coroutine =
            JmapMailboxSet::new(&session, &token, args).expect("create mailbox rename coroutine");
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapMailboxSetResult::Ok {
                    not_updated,
                    keep_alive,
                    ..
                } => {
                    assert!(
                        not_updated.is_empty(),
                        "MAILBOX SET rename: not_updated = {not_updated:?}"
                    );
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break;
                }
                JmapMailboxSetResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapMailboxSetResult::Err { err } => panic!("MAILBOX SET rename: {err}"),
            }
        }
    }

    // ── MAILBOX GET (verify rename) ──────────────────────────────────────────

    {
        let mut coroutine =
            JmapMailboxGet::new(&session, &token, Some(vec![mbox_id.clone()]), None)
                .expect("create mailbox get coroutine");
        let mut arg = None;

        let mailboxes = loop {
            match coroutine.resume(arg.take()) {
                JmapMailboxGetResult::Ok {
                    mailboxes,
                    keep_alive,
                    ..
                } => {
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break mailboxes;
                }
                JmapMailboxGetResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapMailboxGetResult::Err { err } => panic!("MAILBOX GET after rename: {err}"),
            }
        };

        assert_eq!(
            mailboxes[0].name.as_deref(),
            Some(mbox_name_2.as_str()),
            "MAILBOX GET: rename not reflected"
        );
    }

    // ── BLOB UPLOAD ──────────────────────────────────────────────────────────

    let blob_id = {
        let upload_url = Url::parse(&session.upload_url.replace("{accountId}", &account_id))
            .expect("parse upload URL");

        // Reconnect to the upload host if it differs from the API host.
        if upload_url.host_str() != api_url.host_str() {
            stream = connect(&upload_url);
        }

        let eml = build_eml(email).into_bytes();
        let mut coroutine = JmapBlobUpload::new(&token, &upload_url, "message/rfc822", eml);
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapBlobUploadResult::Ok {
                    blob_id,
                    keep_alive,
                    ..
                } => {
                    // Reconnect back to the API host after the upload.
                    if !keep_alive || upload_url.host_str() != api_url.host_str() {
                        stream = connect(&api_url);
                    }
                    break blob_id;
                }
                JmapBlobUploadResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapBlobUploadResult::Err { err } => panic!("BLOB UPLOAD: {err}"),
            }
        }
    };

    // ── EMAIL IMPORT ─────────────────────────────────────────────────────────

    {
        let mut mailbox_ids = BTreeMap::new();
        mailbox_ids.insert(mbox_id.clone(), true);
        let mut emails = BTreeMap::new();
        emails.insert(
            "e1".to_owned(),
            EmailImport {
                blob_id: blob_id.clone(),
                mailbox_ids,
                keywords: None,
                received_at: None,
            },
        );

        let mut coroutine =
            JmapEmailImport::new(&session, &token, emails).expect("create email import coroutine");
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapEmailImportResult::Ok {
                    not_created,
                    keep_alive,
                    ..
                } => {
                    assert!(
                        not_created.is_empty(),
                        "EMAIL IMPORT: not_created = {not_created:?}"
                    );
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break;
                }
                JmapEmailImportResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapEmailImportResult::Err { err } => panic!("EMAIL IMPORT: {err}"),
            }
        }
    }

    // ── EMAIL QUERY ──────────────────────────────────────────────────────────

    let (email_id, thread_id) = {
        let filter = EmailFilter {
            in_mailbox: Some(mbox_id.clone()),
            ..Default::default()
        };

        let mut coroutine =
            JmapEmailQuery::new(&session, &token, Some(filter), None, None, None, None).unwrap();
        let mut arg = None;

        let emails = loop {
            match coroutine.resume(arg.take()) {
                JmapEmailQueryResult::Ok {
                    emails, keep_alive, ..
                } => {
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break emails;
                }
                JmapEmailQueryResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapEmailQueryResult::Err { err } => panic!("EMAIL QUERY: {err}"),
            }
        };

        assert_eq!(emails.len(), 1, "expected exactly one email after import");
        let id = emails[0].id.clone().expect("email id");
        let tid = emails[0].thread_id.clone().expect("thread id");
        (id, tid)
    };

    // ── EMAIL GET ────────────────────────────────────────────────────────────

    {
        let mut coroutine = JmapEmailGet::new(
            &session,
            &token,
            vec![email_id.clone()],
            None,
            false,
            false,
            0,
        )
        .expect("create email get coroutine");
        let mut arg = None;

        let emails = loop {
            match coroutine.resume(arg.take()) {
                JmapEmailGetResult::Ok {
                    emails,
                    not_found,
                    keep_alive,
                    ..
                } => {
                    assert!(not_found.is_empty(), "EMAIL GET: not_found = {not_found:?}");
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break emails;
                }
                JmapEmailGetResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
                JmapEmailGetResult::Err { err } => panic!("EMAIL GET: {err}"),
            }
        };

        assert_eq!(
            emails[0].id.as_deref(),
            Some(email_id.as_str()),
            "EMAIL GET: id mismatch"
        );
    }

    // ── EMAIL SET add $seen ──────────────────────────────────────────────────

    {
        let mut args = JmapEmailSetArgs::default();
        args.set_keyword(&email_id, "$seen");
        let mut coroutine =
            JmapEmailSet::new(&session, &token, args).expect("create email set $seen coroutine");
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapEmailSetResult::Ok {
                    not_updated,
                    keep_alive,
                    ..
                } => {
                    assert!(
                        not_updated.is_empty(),
                        "EMAIL SET $seen: not_updated = {not_updated:?}"
                    );
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break;
                }
                JmapEmailSetResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
                JmapEmailSetResult::Err { err } => panic!("EMAIL SET $seen: {err}"),
            }
        }
    }

    // ── EMAIL SET remove $seen ────────────────────────────────────────────────

    {
        let mut args = JmapEmailSetArgs::default();
        args.unset_keyword(&email_id, "$seen");
        let mut coroutine =
            JmapEmailSet::new(&session, &token, args).expect("create email unset $seen coroutine");
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapEmailSetResult::Ok {
                    not_updated,
                    keep_alive,
                    ..
                } => {
                    assert!(
                        not_updated.is_empty(),
                        "EMAIL SET remove $seen: not_updated = {not_updated:?}"
                    );
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break;
                }
                JmapEmailSetResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
                JmapEmailSetResult::Err { err } => panic!("EMAIL SET remove $seen: {err}"),
            }
        }
    }

    // ── THREAD GET ───────────────────────────────────────────────────────────

    {
        let mut coroutine = JmapThreadGet::new(&session, &token, vec![thread_id.clone()])
            .expect("create thread get coroutine");
        let mut arg = None;

        let threads = loop {
            match coroutine.resume(arg.take()) {
                JmapThreadGetResult::Ok {
                    threads,
                    not_found,
                    keep_alive,
                    ..
                } => {
                    assert!(
                        not_found.is_empty(),
                        "THREAD GET: not_found = {not_found:?}"
                    );
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break threads;
                }
                JmapThreadGetResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapThreadGetResult::Err { err } => panic!("THREAD GET: {err}"),
            }
        };

        assert_eq!(threads[0].id, thread_id, "THREAD GET: id mismatch");
        assert!(
            threads[0].email_ids.contains(&email_id),
            "THREAD GET: email not referenced in thread"
        );
    }

    // ── CLEANUP: destroy email then mailbox ──────────────────────────────────

    {
        let mut args = JmapEmailSetArgs::default();
        args.destroy(&email_id);
        let mut coroutine =
            JmapEmailSet::new(&session, &token, args).expect("create email destroy coroutine");
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapEmailSetResult::Ok {
                    not_destroyed,
                    keep_alive,
                    ..
                } => {
                    assert!(
                        not_destroyed.is_empty(),
                        "EMAIL destroy: not_destroyed = {not_destroyed:?}"
                    );
                    if !keep_alive {
                        stream = connect(&api_url);
                    }
                    break;
                }
                JmapEmailSetResult::Io { input } => arg = Some(handle(&mut stream, input).unwrap()),
                JmapEmailSetResult::Err { err } => panic!("EMAIL destroy: {err}"),
            }
        }
    }

    {
        let args = JmapMailboxSetArgs {
            destroy: Some(vec![mbox_id.clone()]),
            ..Default::default()
        };
        let mut coroutine =
            JmapMailboxSet::new(&session, &token, args).expect("create mailbox destroy coroutine");
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                JmapMailboxSetResult::Ok { not_destroyed, .. } => {
                    assert!(
                        not_destroyed.is_empty(),
                        "MAILBOX destroy: not_destroyed = {not_destroyed:?}"
                    );
                    break;
                }
                JmapMailboxSetResult::Io { input } => {
                    arg = Some(handle(&mut stream, input).unwrap())
                }
                JmapMailboxSetResult::Err { err } => panic!("MAILBOX destroy: {err}"),
            }
        }
    }
}

fn build_eml(email: &str) -> String {
    [
        &format!("From: io-jmap test <{email}>"),
        &format!("To: io-jmap test <{email}>"),
        "Subject: io-jmap integration test",
        "Date: Thu, 01 Jan 2026 00:00:00 +0000",
        "MIME-Version: 1.0",
        "Content-Type: text/plain; charset=utf-8",
        "",
        "This is an automated test email from io-jmap integration tests.",
    ]
    .join("\r\n")
}
