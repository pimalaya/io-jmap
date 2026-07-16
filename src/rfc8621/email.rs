//! JMAP for Mail: Email (RFC 8621 §4).

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

pub mod changes;
pub mod copy;
pub mod get;
pub mod import;
pub mod parse;
pub mod query;
pub mod set;

/// The email has been read.
pub const JMAP_KEYWORD_SEEN: &str = "$seen";

/// The email has been flagged for follow-up.
pub const JMAP_KEYWORD_FLAGGED: &str = "$flagged";

/// The email has been replied to.
pub const JMAP_KEYWORD_ANSWERED: &str = "$answered";

/// The email is a draft.
pub const JMAP_KEYWORD_DRAFT: &str = "$draft";

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
