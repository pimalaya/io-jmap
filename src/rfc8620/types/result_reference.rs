//! JMAP Result Reference (RFC 8620 §7.1).

use serde::Serialize;

/// A JMAP result reference used to back-reference an earlier method
/// call's result within a batch request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResultReference<'a> {
    pub result_of: &'a str,
    pub name: &'static str,
    pub path: &'static str,
}
