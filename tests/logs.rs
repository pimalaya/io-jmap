//! Checks what the library logs of the URLs it requests.
//!
//! Alone in its test binary, the capturing logger being global.

use std::sync::{Arc, Mutex, atomic::AtomicBool};

use io_jmap::rfc8620::{
    blob_download::JmapBlobDownload,
    blob_upload::JmapBlobUpload,
    event_source::{
        JmapCloseAfter,
        subscribe::{JmapEventSource, JmapEventSourceOptions},
    },
    request::JmapBatch,
    send::JmapSend,
    session::JmapSession,
    session_get::JmapSessionGet,
};
use log::{LevelFilter, Log, Metadata, Record};
use secrecy::SecretString;
use url::Url;

static RECORDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Logger keeping what this library logs, not what its dependencies do.
struct Capture;

impl Log for Capture {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if record.target().starts_with("io_jmap") {
            RECORDS.lock().unwrap().push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

#[test]
fn logged_urls_carry_no_userinfo_nor_query() {
    log::set_logger(&Capture).unwrap();
    log::set_max_level(LevelFilter::Trace);

    let secret = "https://user:hunter2@api.example.com/jmap/x?token=s3cr3t";
    let url: Url = secret.parse().unwrap();
    let auth = SecretString::from("Bearer t");
    let session: JmapSession = serde_json::from_value(serde_json::json!({
        "username": "",
        "accounts": {},
        "primaryAccounts": {},
        "capabilities": {},
        "apiUrl": secret,
        "downloadUrl": "",
        "uploadUrl": "",
        "eventSourceUrl": secret,
        "state": "",
    }))
    .unwrap();

    let _ = JmapSessionGet::new(&auth, &url);
    let _ = JmapBlobDownload::new(&auth, &url);
    let _ = JmapBlobUpload::new(&auth, &url, "text/plain", Vec::new());
    let _ = JmapSend::new(&auth, &url, JmapBatch::new().into_request(Vec::new()));
    let _ = JmapEventSource::new(
        &session,
        &auth,
        &[],
        30,
        JmapCloseAfter::No,
        Arc::new(AtomicBool::new(false)),
        JmapEventSourceOptions::default(),
    );

    let records = RECORDS.lock().unwrap();
    let urls: Vec<_> = records
        .iter()
        .filter(|record| record.contains("https://api.example.com/jmap/x"))
        .collect();

    assert_eq!(urls.len(), 5, "{records:?}");

    for record in records.iter() {
        assert!(!record.contains("hunter2"), "{record}");
        assert!(!record.contains("s3cr3t"), "{record}");
    }
}
