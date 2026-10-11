//! Gmail messages: what one carries once it is read, and the content and
//! options used to list, read, send and label them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::GmailAddress;

/// A message as Gmail names it, without its content: a row of a list, and
/// what Gmail answers when a message is sent, labelled or moved to the bin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailMessageRef {
    pub id: String,
    /// The thread it belongs to.
    pub thread_id: Option<String>,
    /// The labels on it, such as `SENT` or `INBOX`. Empty in a list, which
    /// carries ids only.
    pub label_ids: Vec<String>,
}

/// A message that was read, with its MIME parts decoded: the headers that
/// matter, the body as plain text and as HTML, and what is attached.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailMessage {
    pub id: String,
    /// The same on every message of one thread.
    pub thread_id: Option<String>,
    /// The labels on it: `INBOX`, `UNREAD`, `STARRED`, `SENT`, or a label's id.
    pub label_ids: Vec<String>,
    /// A short part of the text, as Gmail shows it in a list.
    pub snippet: Option<String>,
    pub history_id: Option<String>,
    /// When Gmail took the message in, in milliseconds since 1970, written
    /// as a string. It is what the inbox is sorted by.
    pub internal_date: Option<String>,
    /// About how large the whole message is, in bytes.
    pub size_estimate: Option<i64>,
    /// The `From` header: `Grace Hopper <grace@example.test>`.
    pub from: Option<String>,
    /// The `To` header. A header written more than once is joined with commas.
    pub to: Option<String>,
    pub cc: Option<String>,
    /// Present only on a message the account itself wrote.
    pub bcc: Option<String>,
    /// Where the sender asks for replies to go, when that is not `from`. It
    /// is the sender's own text and can be any address at all:
    /// `gmail_messages.reply` never uses it, or `from`, unasked.
    pub reply_to: Option<String>,
    pub subject: Option<String>,
    /// The `Date` header as the sender wrote it: `Fri, 9 Oct 2026 08:15:00 +0000`.
    pub date: Option<String>,
    /// The `Message-ID` header: the id the message carries between mail
    /// systems, in angle brackets.
    pub message_id: Option<String>,
    /// The `Message-ID` of the message this one answers.
    pub in_reply_to: Option<String>,
    /// The `Message-ID`s of the thread before it, oldest first.
    pub references: Option<String>,
    /// The body as plain text. Absent when the message has none.
    pub text: Option<String>,
    /// The body as HTML. Absent when the message has none.
    pub html: Option<String>,
    /// What is attached, without the files themselves.
    pub attachments: Vec<GmailAttachment>,
}

/// A file carried by a message, without its content.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailAttachment {
    /// What `attachment_get` takes. Absent when Gmail sent the content inside
    /// the message and not as a file to fetch.
    pub attachment_id: Option<String>,
    /// The file's name. Empty when the part has none.
    pub filename: String,
    /// The media type, such as `application/pdf`.
    pub mime_type: Option<String>,
    /// The size in bytes.
    pub size: Option<i64>,
    /// Whether the sender marked it to be shown in the body, as a picture in
    /// a signature is, and not offered as a file.
    pub inline: bool,
    /// The name the HTML body refers to an inline part by, as `cid:…`.
    pub content_id: Option<String>,
    /// Where the part sits in the message: `1`, `0.1`.
    pub part_id: Option<String>,
}

/// The content of one attachment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailAttachmentBody {
    /// The size of the file in bytes, before it was encoded.
    pub size: i64,
    /// The file as Gmail sends it: base64 in the URL-safe alphabet.
    pub data: String,
}

/// How much of a message Gmail returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GmailFormat {
    /// Everything: headers, bodies and the list of attachments.
    Full,
    /// Ids, labels and headers, without any body.
    Metadata,
    /// Ids and labels only.
    Minimal,
}

/// Which messages to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailListMessages {
    /// A search in the words of Gmail's own search box: `from:grace
    /// is:unread`, `subject:(q3 plan) newer_than:7d`, `has:attachment`.
    pub q: Option<String>,
    /// Only messages that carry every one of these labels: `INBOX`,
    /// `UNREAD`, `STARRED`, or a label's id.
    pub label_ids: Option<Vec<String>>,
    /// Whether to include what is in Spam and in the bin. Gmail leaves them
    /// out unless this is `true`.
    pub include_spam_trash: Option<bool>,
}

/// How to return one message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailGetMessage {
    /// How much to return: `full` when not given. `metadata` leaves out the
    /// bodies and attachments, `minimal` the headers too.
    pub format: Option<GmailFormat>,
}

/// Labels to put on a message and to take off it. At least one of the two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailModifyMessage {
    /// Labels to add, at most 100: `STARRED` to star, `UNREAD` to mark
    /// unread, `INBOX` to move back to the inbox, or a label's id. Not
    /// `TRASH`, which is `gmail_messages.trash`, and not `SPAM`.
    pub add_label_ids: Option<Vec<String>>,
    /// Labels to remove, at most 100: `INBOX` to archive, `UNREAD` to mark
    /// read, `STARRED` to unstar, or a label's id.
    pub remove_label_ids: Option<Vec<String>>,
}

/// The content of a message that is being written: for sending at once, or
/// for a draft. It goes out from the account's own address.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailSendMessage {
    /// Who it goes to.
    pub to: Option<Vec<GmailAddress>>,
    /// Who receives a copy.
    pub cc: Option<Vec<GmailAddress>>,
    /// Who receives a copy the others are not told about.
    pub bcc: Option<Vec<GmailAddress>>,
    /// One line, in any language.
    pub subject: Option<String>,
    /// The body as plain text.
    pub text: Option<String>,
    /// The body as HTML. Given with `text`, the reader's mail program shows
    /// whichever it prefers; give both so that every program has one to show.
    pub html: Option<String>,
}
