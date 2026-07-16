//! JMAP request/response plumbing (RFC 8620 §3): the Request and Response
//! objects, the batch builder generating call ids, and the result reference
//! used to back-reference an earlier call within a batch.

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A JMAP result reference (RFC 8620 §3.7) used to back-reference an
/// earlier method call's result within a batch request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapResultReference<'a> {
    /// The call id of the method call to reference.
    pub result_of: &'a str,
    /// The name of the referenced method.
    pub name: &'static str,
    /// The JSON pointer into the referenced result.
    pub path: &'static str,
}

/// The JMAP Request object (RFC 8620 §3.3).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapRequest {
    /// Capability URNs required by the methods in this request.
    pub using: Vec<String>,
    /// The method calls to execute, as `(methodName, args, callId)`
    /// tuples.
    pub method_calls: Vec<(String, Value, String)>,
    /// Client-assigned IDs for newly created objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_ids: Option<BTreeMap<String, String>>,
}

/// The JMAP Response object (RFC 8620 §3.4).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapResponse {
    /// Method responses in `(methodName, result, callId)` format.
    ///
    /// If a method failed, `methodName` is `"error"` and `result` is a
    /// [`crate::rfc8620::error::JmapMethodError`] object.
    pub method_responses: Vec<(String, Value, String)>,
    /// Server-assigned IDs for objects created by this request.
    #[serde(default)]
    pub created_ids: Option<BTreeMap<String, String>>,
    /// The current state of the session after this request.
    pub session_state: String,
}

/// Builder for batched JMAP requests: multiple method calls in one HTTP
/// request, with generated call IDs for [`JmapResultReference`] back-refs.
#[derive(Debug, Default)]
pub struct JmapBatch {
    calls: Vec<(String, Value, String)>,
    counter: usize,
}

impl JmapBatch {
    /// Creates a new empty batch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a method call. Returns the call ID (`"c0"`, `"c1"`, …) for use in
    /// back-references from later calls.
    pub fn add(&mut self, method: impl Into<String>, args: Value) -> String {
        let call_id = format!("c{}", self.counter);
        self.counter += 1;
        self.calls.push((method.into(), args, call_id.clone()));
        call_id
    }

    /// Consumes the batch and returns a [`JmapRequest`].
    pub fn into_request(self, using: Vec<String>) -> JmapRequest {
        JmapRequest {
            using,
            method_calls: self.calls,
            created_ids: None,
        }
    }
}
