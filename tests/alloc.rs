//! Counts what decoding a method response allocates.
//!
//! Alone in its test binary, the counting allocator being global. The
//! count is kept per thread, so the harness threads do not disturb it.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    fmt::Write,
};

use io_jmap::{
    coroutine::*,
    rfc8620::session::JmapSession,
    rfc8621::email::get::{JmapEmailGet, JmapEmailGetOptions},
};
use secrecy::SecretString;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

// SAFETY: every call is handed to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: the caller upholds the contract of `alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds the contract of `dealloc`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: the caller upholds the contract of `realloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const EMAILS: usize = 1000;

/// One `Email/get` response of [`EMAILS`] emails, each with an id and 20
/// properties the library does not model.
fn reply() -> Vec<u8> {
    let mut list = String::new();

    for email in 0..EMAILS {
        let sep = if email == 0 { "" } else { "," };
        write!(list, r#"{sep}{{"id":"e{email}""#).unwrap();

        for property in 0..20 {
            write!(list, r#","x{property}":"some value {property}""#).unwrap();
        }

        list.push('}');
    }

    let body = format!(
        r#"{{"methodResponses":[["Email/get",{{"accountId":"a1","state":"s1","list":[{list}],"notFound":[]}},"c0"]],"sessionState":"s1"}}"#
    );

    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[test]
fn unmodelled_properties_cost_no_allocation() {
    let session: JmapSession = serde_json::from_str(
        r#"{
            "username": "",
            "accounts": {},
            "primaryAccounts": {"urn:ietf:params:jmap:mail": "a1"},
            "capabilities": {},
            "apiUrl": "https://api.example.com/jmap/",
            "downloadUrl": "",
            "uploadUrl": "",
            "eventSourceUrl": "",
            "state": ""
        }"#,
    )
    .unwrap();
    let auth = SecretString::from("Bearer t");
    let opts = JmapEmailGetOptions::default();
    let mut coroutine = JmapEmailGet::new(&session, &auth, Vec::new(), opts).unwrap();
    let reply = reply();

    assert!(matches!(
        coroutine.resume(None),
        JmapCoroutineState::Yielded(JmapYield::WantsWrite(_))
    ));
    assert!(matches!(
        coroutine.resume(None),
        JmapCoroutineState::Yielded(JmapYield::WantsRead)
    ));

    let before = ALLOCATIONS.get();
    let state = coroutine.resume(Some(&reply));
    let allocations = ALLOCATIONS.get() - before;

    let JmapCoroutineState::Complete(Ok(out)) = state else {
        panic!("the response did not decode");
    };
    assert_eq!(out.emails.len(), EMAILS);

    // NOTE: one string per email id, plus the buffers; a JSON tree of the
    // response costs more than 40 allocations per email.
    assert!(
        allocations < 2 * EMAILS,
        "{allocations} allocations for {EMAILS} emails"
    );
}
