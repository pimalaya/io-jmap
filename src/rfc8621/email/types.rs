//! JMAP Email types (RFC 8621 §4).

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

/// A JMAP Email object (RFC 8621 §4.1).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmail {
    pub id: Option<String>,
    /// Blob ID for the raw RFC 5322 message.
    pub blob_id: Option<String>,
    pub thread_id: Option<String>,
    /// `{ mailbox-id -> true }` for each mailbox containing the email.
    pub mailbox_ids: Option<BTreeMap<String, bool>>,
    /// `{ keyword -> true }`. Standard: `$seen`, `$flagged`, `$answered`,
    /// `$draft`.
    pub keywords: Option<BTreeMap<String, bool>>,
    /// Size of the raw RFC 5322 message, in bytes.
    pub size: Option<u64>,
    /// RFC 3339 receive time.
    pub received_at: Option<String>,
    pub message_id: Option<Vec<String>>,
    pub in_reply_to: Option<Vec<String>>,
    pub references: Option<Vec<String>>,
    pub sender: Option<Vec<JmapEmailAddress>>,
    pub from: Option<Vec<JmapEmailAddress>>,
    pub to: Option<Vec<JmapEmailAddress>>,
    pub cc: Option<Vec<JmapEmailAddress>>,
    pub bcc: Option<Vec<JmapEmailAddress>>,
    pub reply_to: Option<Vec<JmapEmailAddress>>,
    pub subject: Option<String>,
    /// `Date` header as an RFC 3339 string.
    pub sent_at: Option<String>,
    pub body_structure: Option<JmapEmailBodyPart>,
    /// `{ part-id -> body }` for text parts.
    pub body_values: Option<BTreeMap<String, JmapEmailBodyValue>>,
    pub text_body: Option<Vec<JmapEmailBodyPart>>,
    pub html_body: Option<Vec<JmapEmailBodyPart>>,
    pub attachments: Option<Vec<JmapEmailBodyPart>>,
    pub has_attachment: Option<bool>,
    /// Short plaintext preview (up to 256 chars).
    pub preview: Option<String>,
    /// Raw headers in order of appearance.
    pub headers: Option<Vec<JmapEmailHeader>>,
}

/// An email address (name + email pair).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailAddress {
    pub name: Option<String>,
    pub email: String,
}

/// A raw email header name-value pair.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailHeader {
    /// Field name, without trailing colon.
    pub name: String,
    /// Raw value, with leading whitespace preserved.
    pub value: String,
}

/// A MIME body part descriptor.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailBodyPart {
    pub part_id: Option<String>,
    pub blob_id: Option<String>,
    pub size: Option<u64>,
    /// Filename from `Content-Disposition` or `Content-Type`.
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub charset: Option<String>,
    /// `inline` or `attachment`.
    pub disposition: Option<String>,
    pub cid: Option<String>,
    pub language: Option<Vec<String>>,
    pub location: Option<String>,
    /// Sub-parts (multipart only).
    pub sub_parts: Option<Vec<JmapEmailBodyPart>>,
    pub headers: Option<Vec<JmapEmailHeader>>,
}

/// The text content of a body part.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailBodyValue {
    pub value: String,
    /// Charset or encoding problem during decode.
    pub is_encoding_problem: bool,
    /// Whether the value was truncated.
    pub is_truncated: bool,
}

/// [`JmapEmail`] properties requestable in `Email/get` (RFC 8621 §4.1).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JmapEmailProperty {
    Id,
    BlobId,
    ThreadId,
    MailboxIds,
    Keywords,
    Size,
    ReceivedAt,
    MessageId,
    InReplyTo,
    References,
    Sender,
    From,
    To,
    Cc,
    Bcc,
    ReplyTo,
    Subject,
    SentAt,
    BodyStructure,
    BodyValues,
    TextBody,
    HtmlBody,
    Attachments,
    HasAttachment,
    Preview,
    Headers,
}

/// Sort property for `Email/query` (RFC 8621 §4.4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JmapEmailSortProperty {
    ReceivedAt,
    SentAt,
    Size,
    From,
    To,
    Subject,
    HasAttachment,
    /// Sort by keyword presence on the email (requires `keyword` field).
    Keyword,
    /// Sort by whether all emails in the thread have a keyword
    /// (requires `keyword` field).
    AllInThreadHaveKeyword,
    /// Sort by whether some emails in the thread have a keyword
    /// (requires `keyword` field).
    SomeInThreadHaveKeyword,
}

/// JmapFilter for `Email/query` (RFC 8621 §4.4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_mailbox: Option<String>,
    /// Exclude messages in any of these mailbox IDs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_mailbox_other_than: Option<Vec<String>>,
    /// RFC 3339 upper bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// RFC 3339 lower bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_in_thread_have_keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub some_in_thread_have_keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub none_in_thread_have_keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_attachment: Option<bool>,
    /// Full-text search query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// Comparator for `Email/query` sorting (RFC 8621 §4.4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailComparator {
    pub property: JmapEmailSortProperty,
    /// Ascending if `None` or `Some(true)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,
    /// String comparison collation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,
    /// Required when `property` is `Keyword`, `AllInThreadHaveKeyword`, or
    /// `SomeInThreadHaveKeyword`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
}

impl JmapEmailComparator {
    /// Sort by `receivedAt` descending (newest first).
    pub fn received_at_desc() -> Self {
        Self {
            property: JmapEmailSortProperty::ReceivedAt,
            is_ascending: Some(false),
            collation: None,
            keyword: None,
        }
    }
}

/// A single operation in an `Email/set` update patch (RFC 8621 §4.7). Each
/// variant serialises as a JSON Pointer entry in a flat patch object.
#[derive(Clone, Debug)]
pub enum JmapEmailPatchOp {
    /// Set a keyword: `"keywords/<kw>": true`
    SetKeyword(String),
    /// Unset a keyword: `"keywords/<kw>": null`
    UnsetKeyword(String),
    /// Replace all keywords atomically: `"keywords": { ... }`
    ReplaceKeywords(BTreeMap<String, bool>),
    /// Add email to a mailbox: `"mailboxIds/<id>": true`
    AddToMailbox(String),
    /// Remove email from a mailbox: `"mailboxIds/<id>": null`
    RemoveFromMailbox(String),
    /// Replace mailbox membership atomically: `"mailboxIds": { ... }`
    ReplaceMailboxIds(BTreeMap<String, bool>),
}

/// A set of patch operations applied to a single email in `Email/set`.
///
/// Serializes to a flat JSON Merge Patch object (RFC 7396).
#[derive(Clone, Debug, Default)]
pub struct JmapEmailPatch(pub Vec<JmapEmailPatchOp>);

impl JmapEmailPatch {
    pub fn set_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::SetKeyword(keyword.into()));
        self
    }

    pub fn unset_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::UnsetKeyword(keyword.into()));
        self
    }

    pub fn replace_keywords(mut self, keywords: BTreeMap<String, bool>) -> Self {
        self.0.push(JmapEmailPatchOp::ReplaceKeywords(keywords));
        self
    }

    pub fn add_to_mailbox(mut self, id: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::AddToMailbox(id.into()));
        self
    }

    pub fn remove_from_mailbox(mut self, id: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::RemoveFromMailbox(id.into()));
        self
    }

    pub fn replace_mailbox_ids(mut self, ids: BTreeMap<String, bool>) -> Self {
        self.0.push(JmapEmailPatchOp::ReplaceMailboxIds(ids));
        self
    }
}

impl Serialize for JmapEmailPatch {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for op in &self.0 {
            match op {
                JmapEmailPatchOp::SetKeyword(kw) => {
                    map.serialize_entry(&format!("keywords/{kw}"), &true)?
                }
                JmapEmailPatchOp::UnsetKeyword(kw) => {
                    map.serialize_entry(&format!("keywords/{kw}"), &Option::<bool>::None)?
                }
                JmapEmailPatchOp::ReplaceKeywords(kws) => map.serialize_entry("keywords", kws)?,
                JmapEmailPatchOp::AddToMailbox(id) => {
                    map.serialize_entry(&format!("mailboxIds/{id}"), &true)?
                }
                JmapEmailPatchOp::RemoveFromMailbox(id) => {
                    map.serialize_entry(&format!("mailboxIds/{id}"), &Option::<bool>::None)?
                }
                JmapEmailPatchOp::ReplaceMailboxIds(ids) => {
                    map.serialize_entry("mailboxIds", ids)?
                }
            }
        }
        map.end()
    }
}

/// Arguments for importing a single RFC 5322 message via `Email/import`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailImportArgs {
    /// Blob ID of the RFC 5322 message.
    pub blob_id: String,
    /// `{ mailbox-id -> true }` for destination mailboxes.
    pub mailbox_ids: BTreeMap<String, bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<BTreeMap<String, bool>>,
    /// RFC 3339 override for `receivedAt`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_at: Option<String>,
}

/// Arguments for copying a single email between accounts via `Email/copy`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailCopyArgs {
    /// Source email ID.
    pub id: String,
    /// `{ mailbox-id -> true }` for destination mailboxes.
    pub mailbox_ids: BTreeMap<String, bool>,
    /// Keywords on the copy (replaces source keywords).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<BTreeMap<String, bool>>,
    /// RFC 3339 override for the copy's `receivedAt`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_at: Option<String>,
}

/// Per-object error returned in `Email/set` responses (RFC 8621 §4.7).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapEmailSetItemError {
    /// The email would exceed the server's keyword limit (RFC 8621 §4.7).
    TooManyKeywords { description: Option<String> },
    /// The email would be in too many mailboxes (RFC 8621 §4.7).
    TooManyMailboxes { description: Option<String> },
    /// One or more blob IDs in the email were not found (RFC 8621 §4.7).
    BlobNotFound { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): patch could not be applied.
    InvalidPatch { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): would destroy an object already
    /// queued for destruction in the same request.
    WillDestroy { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): one or more properties were invalid.
    InvalidProperties {
        description: Option<String>,
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Standard set error (RFC 8620 §5.3): tried to create/destroy a
    /// server-managed singleton.
    Singleton { description: Option<String> },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}

/// Per-object error returned in `Email/import` responses (RFC 8621 §4.9).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapEmailImportItemError {
    /// The message body was not a valid RFC 5322 message (RFC 8621 §4.9).
    InvalidEmail { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): one or more properties were invalid.
    InvalidProperties {
        description: Option<String>,
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}

/// Per-object error returned in `Email/copy` responses (RFC 8621 §4.10).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapEmailCopyItemError {
    /// The email already exists in the destination account (RFC 8621 §4.10).
    AlreadyExists { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound { description: Option<String> },
    /// Standard set error (RFC 8620 §5.3): one or more properties were invalid.
    InvalidProperties {
        description: Option<String>,
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}
