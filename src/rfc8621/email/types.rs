//! JMAP Email types (RFC 8621 §4).

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

/// A JMAP Email object (RFC 8621 §4.1).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmail {
    /// The server-assigned email id.
    pub id: Option<String>,
    /// Blob ID for the raw RFC 5322 message.
    pub blob_id: Option<String>,
    /// The id of the thread the email belongs to.
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
    /// The `Message-ID` header values.
    pub message_id: Option<Vec<String>>,
    /// The `In-Reply-To` header values.
    pub in_reply_to: Option<Vec<String>>,
    /// The `References` header values.
    pub references: Option<Vec<String>>,
    /// The `Sender` header addresses.
    pub sender: Option<Vec<JmapEmailAddress>>,
    /// The `From` header addresses.
    pub from: Option<Vec<JmapEmailAddress>>,
    /// The `To` header addresses.
    pub to: Option<Vec<JmapEmailAddress>>,
    /// The `Cc` header addresses.
    pub cc: Option<Vec<JmapEmailAddress>>,
    /// The `Bcc` header addresses.
    pub bcc: Option<Vec<JmapEmailAddress>>,
    /// The `Reply-To` header addresses.
    pub reply_to: Option<Vec<JmapEmailAddress>>,
    /// The `Subject` header value.
    pub subject: Option<String>,
    /// `Date` header as an RFC 3339 string.
    pub sent_at: Option<String>,
    /// The full MIME structure of the message.
    pub body_structure: Option<JmapEmailBodyPart>,
    /// `{ part-id -> body }` for text parts.
    pub body_values: Option<BTreeMap<String, JmapEmailBodyValue>>,
    /// The text/plain parts to display as the message body.
    pub text_body: Option<Vec<JmapEmailBodyPart>>,
    /// The text/html parts to display as the message body.
    pub html_body: Option<Vec<JmapEmailBodyPart>>,
    /// The parts to display as attachments.
    pub attachments: Option<Vec<JmapEmailBodyPart>>,
    /// Whether any part is an attachment.
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
    /// The display name, when present.
    pub name: Option<String>,
    /// The address itself (local@domain).
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
    /// Part id, scoped to the message.
    pub part_id: Option<String>,
    /// Blob id to download the decoded part content.
    pub blob_id: Option<String>,
    /// Size of the decoded part content, in bytes.
    pub size: Option<u64>,
    /// Filename from `Content-Disposition` or `Content-Type`.
    pub name: Option<String>,
    /// The `Content-Type` media type.
    pub r#type: Option<String>,
    /// The `Content-Type` charset parameter.
    pub charset: Option<String>,
    /// `inline` or `attachment`.
    pub disposition: Option<String>,
    /// The `Content-Id` value, without angle brackets.
    pub cid: Option<String>,
    /// The `Content-Language` values.
    pub language: Option<Vec<String>>,
    /// The `Content-Location` value.
    pub location: Option<String>,
    /// Sub-parts (multipart only).
    pub sub_parts: Option<Vec<JmapEmailBodyPart>>,
    /// Raw headers of the part.
    pub headers: Option<Vec<JmapEmailHeader>>,
}

/// The text content of a body part.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailBodyValue {
    /// The decoded text content.
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
    /// The `id` property.
    Id,
    /// The `blobId` property.
    BlobId,
    /// The `threadId` property.
    ThreadId,
    /// The `mailboxIds` property.
    MailboxIds,
    /// The `keywords` property.
    Keywords,
    /// The `size` property.
    Size,
    /// The `receivedAt` property.
    ReceivedAt,
    /// The `messageId` property.
    MessageId,
    /// The `inReplyTo` property.
    InReplyTo,
    /// The `references` property.
    References,
    /// The `sender` property.
    Sender,
    /// The `from` property.
    From,
    /// The `to` property.
    To,
    /// The `cc` property.
    Cc,
    /// The `bcc` property.
    Bcc,
    /// The `replyTo` property.
    ReplyTo,
    /// The `subject` property.
    Subject,
    /// The `sentAt` property.
    SentAt,
    /// The `bodyStructure` property.
    BodyStructure,
    /// The `bodyValues` property.
    BodyValues,
    /// The `textBody` property.
    TextBody,
    /// The `htmlBody` property.
    HtmlBody,
    /// The `attachments` property.
    Attachments,
    /// The `hasAttachment` property.
    HasAttachment,
    /// The `preview` property.
    Preview,
    /// The `headers` property.
    Headers,
}

/// Sort property for `Email/query` (RFC 8621 §4.4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JmapEmailSortProperty {
    /// Sort by receive time.
    ReceivedAt,
    /// Sort by the `Date` header.
    SentAt,
    /// Sort by message size.
    Size,
    /// Sort by the first `From` address.
    From,
    /// Sort by the first `To` address.
    To,
    /// Sort by the base subject.
    Subject,
    /// Sort by attachment presence.
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

/// Filter condition for `Email/query` (RFC 8621 §4.4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailFilter {
    /// Only messages in this mailbox ID.
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
    /// Only messages of at least this size, in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_size: Option<u64>,
    /// Only messages strictly below this size, in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    /// Only threads where every email carries this keyword.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_in_thread_have_keyword: Option<String>,
    /// Only threads where at least one email carries this keyword.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub some_in_thread_have_keyword: Option<String>,
    /// Only threads where no email carries this keyword.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub none_in_thread_have_keyword: Option<String>,
    /// Only messages carrying this keyword.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_keyword: Option<String>,
    /// Only messages not carrying this keyword.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_keyword: Option<String>,
    /// Only messages with (or without) attachments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_attachment: Option<bool>,
    /// Full-text search query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Text search over the `From` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Text search over the `To` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Text search over the `Cc` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<String>,
    /// Text search over the `Bcc` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<String>,
    /// Text search over the `Subject` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Text search over the message body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// Comparator for `Email/query` sorting (RFC 8621 §4.4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JmapEmailComparator {
    /// The property to sort by.
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
    /// Appends a [`JmapEmailPatchOp::SetKeyword`] operation.
    pub fn set_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::SetKeyword(keyword.into()));
        self
    }

    /// Appends a [`JmapEmailPatchOp::UnsetKeyword`] operation.
    pub fn unset_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::UnsetKeyword(keyword.into()));
        self
    }

    /// Appends a [`JmapEmailPatchOp::ReplaceKeywords`] operation.
    pub fn replace_keywords(mut self, keywords: BTreeMap<String, bool>) -> Self {
        self.0.push(JmapEmailPatchOp::ReplaceKeywords(keywords));
        self
    }

    /// Appends a [`JmapEmailPatchOp::AddToMailbox`] operation.
    pub fn add_to_mailbox(mut self, id: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::AddToMailbox(id.into()));
        self
    }

    /// Appends a [`JmapEmailPatchOp::RemoveFromMailbox`] operation.
    pub fn remove_from_mailbox(mut self, id: impl Into<String>) -> Self {
        self.0.push(JmapEmailPatchOp::RemoveFromMailbox(id.into()));
        self
    }

    /// Appends a [`JmapEmailPatchOp::ReplaceMailboxIds`] operation.
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
    /// `{ keyword -> true }` to set on the imported email.
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
    TooManyKeywords {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// The email would be in too many mailboxes (RFC 8621 §4.7).
    TooManyMailboxes {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// One or more blob IDs in the email were not found (RFC 8621 §4.7).
    BlobNotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): patch could not be applied.
    InvalidPatch {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): would destroy an object already
    /// queued for destruction in the same request.
    WillDestroy {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): one or more properties were invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Standard set error (RFC 8620 §5.3): tried to create/destroy a
    /// server-managed singleton.
    Singleton {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}

/// Per-object error returned in `Email/import` responses (RFC 8621 §4.9).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JmapEmailImportItemError {
    /// The message body was not a valid RFC 5322 message (RFC 8621 §4.9).
    InvalidEmail {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): one or more properties were invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
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
    AlreadyExists {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): target id not found.
    NotFound {
        /// Optional human-readable detail.
        description: Option<String>,
    },
    /// Standard set error (RFC 8620 §5.3): one or more properties were invalid.
    InvalidProperties {
        /// Optional human-readable detail.
        description: Option<String>,
        /// The invalid property names.
        #[serde(default)]
        properties: Vec<String>,
    },
    /// Catch-all for set errors not modelled above.
    #[serde(other)]
    Unknown,
}
