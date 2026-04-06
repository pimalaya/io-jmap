//! JMAP Email types (RFC 8621 §4).

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

/// A JMAP Email object (RFC 8621 §4.1).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Email {
    /// Server-assigned ID.
    pub id: Option<String>,

    /// The ID of the blob containing the raw RFC 5322 message.
    pub blob_id: Option<String>,

    /// The ID of the thread this email belongs to.
    pub thread_id: Option<String>,

    /// Map of mailbox ID to `true` for each mailbox containing this email.
    pub mailbox_ids: Option<BTreeMap<String, bool>>,

    /// Map of keyword to `true` for each keyword set on this email.
    ///
    /// Standard keywords: `$seen`, `$flagged`, `$answered`, `$draft`.
    pub keywords: Option<BTreeMap<String, bool>>,

    /// Size in bytes of the raw RFC 5322 message.
    pub size: Option<u64>,

    /// Date/time the email was received by the server (RFC 3339).
    pub received_at: Option<String>,

    /// The `Message-ID` header value(s).
    pub message_id: Option<Vec<String>>,

    /// The `In-Reply-To` header value(s).
    pub in_reply_to: Option<Vec<String>>,

    /// The `References` header value(s).
    pub references: Option<Vec<String>>,

    /// The `Sender` header value(s).
    pub sender: Option<Vec<EmailAddress>>,

    /// The `From` header value(s).
    pub from: Option<Vec<EmailAddress>>,

    /// The `To` header value(s).
    pub to: Option<Vec<EmailAddress>>,

    /// The `Cc` header value(s).
    pub cc: Option<Vec<EmailAddress>>,

    /// The `Bcc` header value(s).
    pub bcc: Option<Vec<EmailAddress>>,

    /// The `Reply-To` header value(s).
    pub reply_to: Option<Vec<EmailAddress>>,

    /// The `Subject` header value.
    pub subject: Option<String>,

    /// The `Date` header value as an RFC 3339 date-time string.
    pub sent_at: Option<String>,

    /// Body part structure of the email.
    pub body_structure: Option<EmailBodyPart>,

    /// Map of part ID to body value for text parts.
    pub body_values: Option<BTreeMap<String, EmailBodyValue>>,

    /// List of text body parts.
    pub text_body: Option<Vec<EmailBodyPart>>,

    /// List of HTML body parts.
    pub html_body: Option<Vec<EmailBodyPart>>,

    /// List of attachment body parts.
    pub attachments: Option<Vec<EmailBodyPart>>,

    /// Whether the email has at least one attachment.
    pub has_attachment: Option<bool>,

    /// Short plaintext preview of the email body (up to 256 chars).
    pub preview: Option<String>,

    /// Raw header list in order of appearance.
    pub headers: Option<Vec<EmailHeader>>,
}

/// An email address (name + email pair).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailAddress {
    /// The display name, or `null` if not present.
    pub name: Option<String>,

    /// The email address.
    pub email: String,
}

/// A raw email header name-value pair.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailHeader {
    /// Header field name (without trailing colon).
    pub name: String,

    /// Raw header value (with leading whitespace preserved).
    pub value: String,
}

/// A MIME body part descriptor.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailBodyPart {
    /// Part ID (used to look up body values).
    pub part_id: Option<String>,

    /// Blob ID for this part (for downloading).
    pub blob_id: Option<String>,

    /// Size in bytes.
    pub size: Option<u64>,

    /// Filename from `Content-Disposition` or `Content-Type`.
    pub name: Option<String>,

    /// MIME type (e.g. `text/plain`, `text/html`).
    pub r#type: Option<String>,

    /// Charset.
    pub charset: Option<String>,

    /// Content disposition (`inline` or `attachment`).
    pub disposition: Option<String>,

    /// Content-ID.
    pub cid: Option<String>,

    /// Content-Language values.
    pub language: Option<Vec<String>>,

    /// Content-Location URL.
    pub location: Option<String>,

    /// Sub-parts (for multipart types).
    pub sub_parts: Option<Vec<EmailBodyPart>>,

    /// Headers for this body part.
    pub headers: Option<Vec<EmailHeader>>,
}

/// The text content of a body part.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailBodyValue {
    /// The text content.
    pub value: String,

    /// Whether there was a charset or encoding problem.
    pub is_encoding_problem: bool,

    /// Whether the value was truncated.
    pub is_truncated: bool,
}

/// Properties of an [`Email`] object that can be requested in `Email/get`.
///
/// All properties are defined in RFC 8621 §4.1.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EmailProperty {
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
pub enum EmailSortProperty {
    ReceivedAt,
    SentAt,
    Size,
    From,
    To,
    Subject,
    HasAttachment,
    /// Sort by keyword presence on the email (requires `keyword` field).
    Keyword,
    /// Sort by whether all emails in the thread have a keyword (requires `keyword` field).
    AllInThreadHaveKeyword,
    /// Sort by whether some emails in the thread have a keyword (requires `keyword` field).
    SomeInThreadHaveKeyword,
}

/// Filter for `Email/query` (RFC 8621 §4.4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailFilter {
    /// Filter by mailbox ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_mailbox: Option<String>,

    /// Exclude messages in any of these mailbox IDs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_mailbox_other_than: Option<Vec<String>>,

    /// Filter by `before` date (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,

    /// Filter by `after` date (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,

    /// Filter by minimum size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_size: Option<u64>,

    /// Filter by maximum size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,

    /// Filter by thread that all emails are in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_in_thread_have_keyword: Option<String>,

    /// Filter by thread that some emails are in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub some_in_thread_have_keyword: Option<String>,

    /// Filter by thread that no emails are in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub none_in_thread_have_keyword: Option<String>,

    /// Filter by keyword on the email itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_keyword: Option<String>,

    /// Filter by absence of keyword.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_keyword: Option<String>,

    /// Filter by attachment presence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_attachment: Option<bool>,

    /// Full-text search query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Search in the `From` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,

    /// Search in the `To` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,

    /// Search in the `Cc` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<String>,

    /// Search in the `Bcc` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<String>,

    /// Search in the `Subject` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,

    /// Search in the email body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// Comparator for `Email/query` sorting (RFC 8621 §4.4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailComparator {
    /// The property to sort by.
    pub property: EmailSortProperty,

    /// Whether to sort in ascending order. Defaults to `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ascending: Option<bool>,

    /// The collation to use for string comparison.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,

    /// Keyword to sort by (required when `property` is `Keyword`,
    /// `AllInThreadHaveKeyword`, or `SomeInThreadHaveKeyword`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
}

impl EmailComparator {
    /// Sort by `receivedAt` descending (newest first).
    pub fn received_at_desc() -> Self {
        Self {
            property: EmailSortProperty::ReceivedAt,
            is_ascending: Some(false),
            collation: None,
            keyword: None,
        }
    }
}

/// A single operation in an `Email/set` update patch (RFC 8621 §4.7).
///
/// Each variant serializes to a JSON Pointer path → value entry in the
/// flat patch object sent to the server.
#[derive(Clone, Debug)]
pub enum EmailPatchOp {
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
pub struct EmailPatch(pub Vec<EmailPatchOp>);

impl EmailPatch {
    pub fn set_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.push(EmailPatchOp::SetKeyword(keyword.into()));
        self
    }

    pub fn unset_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.push(EmailPatchOp::UnsetKeyword(keyword.into()));
        self
    }

    pub fn replace_keywords(mut self, keywords: BTreeMap<String, bool>) -> Self {
        self.0.push(EmailPatchOp::ReplaceKeywords(keywords));
        self
    }

    pub fn add_to_mailbox(mut self, id: impl Into<String>) -> Self {
        self.0.push(EmailPatchOp::AddToMailbox(id.into()));
        self
    }

    pub fn remove_from_mailbox(mut self, id: impl Into<String>) -> Self {
        self.0.push(EmailPatchOp::RemoveFromMailbox(id.into()));
        self
    }

    pub fn replace_mailbox_ids(mut self, ids: BTreeMap<String, bool>) -> Self {
        self.0.push(EmailPatchOp::ReplaceMailboxIds(ids));
        self
    }
}

impl Serialize for EmailPatch {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for op in &self.0 {
            match op {
                EmailPatchOp::SetKeyword(kw) => {
                    map.serialize_entry(&format!("keywords/{kw}"), &true)?
                }
                EmailPatchOp::UnsetKeyword(kw) => {
                    map.serialize_entry(&format!("keywords/{kw}"), &Option::<bool>::None)?
                }
                EmailPatchOp::ReplaceKeywords(kws) => map.serialize_entry("keywords", kws)?,
                EmailPatchOp::AddToMailbox(id) => {
                    map.serialize_entry(&format!("mailboxIds/{id}"), &true)?
                }
                EmailPatchOp::RemoveFromMailbox(id) => {
                    map.serialize_entry(&format!("mailboxIds/{id}"), &Option::<bool>::None)?
                }
                EmailPatchOp::ReplaceMailboxIds(ids) => map.serialize_entry("mailboxIds", ids)?,
            }
        }
        map.end()
    }
}

/// Arguments for importing a single RFC 5322 message via `Email/import`.
///
/// Used in `Email/import` to import a blob (previously uploaded) into
/// one or more mailboxes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailImport {
    /// Blob ID of the RFC 5322 message to import.
    pub blob_id: String,

    /// Map of mailbox ID → `true` for mailboxes to place the email in.
    pub mailbox_ids: BTreeMap<String, bool>,

    /// Keywords to set on the imported email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<BTreeMap<String, bool>>,

    /// Override the `receivedAt` time (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_at: Option<String>,
}

/// Arguments for copying a single email via `Email/copy`.
///
/// Used in `Email/copy` to copy an email between accounts.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailCopy {
    /// ID of the email to copy.
    pub id: String,

    /// Map of mailbox ID → `true` for destination mailboxes.
    pub mailbox_ids: BTreeMap<String, bool>,

    /// Keywords to set on the copy (replaces source keywords).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<BTreeMap<String, bool>>,

    /// Override the `receivedAt` time on the copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_at: Option<String>,
}

/// Standard JMAP email keywords.
pub mod keywords {
    /// The email has been read.
    pub const SEEN: &str = "$seen";
    /// The email has been flagged for follow-up.
    pub const FLAGGED: &str = "$flagged";
    /// The email has been replied to.
    pub const ANSWERED: &str = "$answered";
    /// The email is a draft.
    pub const DRAFT: &str = "$draft";
}
