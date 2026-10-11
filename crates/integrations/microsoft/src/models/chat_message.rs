//! Messages in a channel or a chat: what Graph returns for them, and the content used to send one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ItemBody;
use super::nullable::nullable;

/// A message in a channel or a chat.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    /// The id of the message this one replies to, in a channel.
    pub reply_to_id: Option<String>,
    /// `message` for what someone wrote; `systemEventMessage` for what Teams
    /// itself noted, such as a member being added. Such a message has no
    /// sender and nothing to read.
    pub message_type: Option<String>,
    pub created_date_time: Option<String>,
    pub last_modified_date_time: Option<String>,
    /// Set when the message was edited after it was sent.
    pub last_edited_date_time: Option<String>,
    /// Set when the message was deleted. Its body is then empty.
    pub deleted_date_time: Option<String>,
    pub subject: Option<String>,
    /// `normal`, `high` or `urgent`.
    pub importance: Option<String>,
    /// The address that opens the message in Teams.
    pub web_url: Option<String>,
    /// The chat the message is in, for a message in a chat.
    pub chat_id: Option<String>,
    /// The team and channel the message is in, for a message in a channel.
    pub channel_identity: Option<ChannelIdentity>,
    /// Who sent it: a person, an application or a bot.
    pub from: Option<MessageSender>,
    /// The message as it was written: HTML with `<at>` tags for mentions and
    /// `<attachment>` tags where attachments sit.
    pub body: Option<ItemBody>,
    /// The body as plain text, with each mention written as `@` and the name.
    /// Socket writes this; Graph does not send it.
    pub text: String,
    #[serde(deserialize_with = "nullable")]
    pub attachments: Vec<ChatAttachment>,
    #[serde(deserialize_with = "nullable")]
    pub mentions: Vec<Mention>,
    #[serde(deserialize_with = "nullable")]
    pub reactions: Vec<Reaction>,
}

/// The team and channel a channel message is in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ChannelIdentity {
    pub team_id: Option<String>,
    pub channel_id: Option<String>,
}

/// Who sent a message. One of the three is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MessageSender {
    pub user: Option<SenderIdentity>,
    /// An application, or a bot when `applicationIdentityType` is `bot`.
    pub application: Option<SenderIdentity>,
    pub device: Option<SenderIdentity>,
}

/// One sender.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SenderIdentity {
    pub id: Option<String>,
    pub display_name: Option<String>,
    /// For a person: `aadUser`, `onPremiseAadUser`, `anonymousGuest`, `federatedUser` and so on.
    pub user_identity_type: Option<String>,
    /// For an application: `bot`, `aadApplication`, `tenantBot`, `office365Connector` or `outgoingWebhook`.
    pub application_identity_type: Option<String>,
}

/// Something attached to a message: a file, a card, or a quoted message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ChatAttachment {
    /// The id the body's `<attachment>` tag refers to.
    pub id: Option<String>,
    /// `reference` for a file, or a card's media type.
    pub content_type: Option<String>,
    /// Where a file is.
    pub content_url: Option<String>,
    /// A card's own content, as JSON in a string.
    pub content: Option<String>,
    pub name: Option<String>,
    pub thumbnail_url: Option<String>,
}

/// Someone mentioned in a message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Mention {
    /// The number the body's `<at id="…">` tag refers to.
    pub id: Option<i64>,
    /// The name as it is shown in the message.
    pub mention_text: Option<String>,
    pub mentioned: Option<Mentioned>,
}

/// Who or what a mention is of. One of the three is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Mentioned {
    pub user: Option<SenderIdentity>,
    pub application: Option<SenderIdentity>,
    /// A whole team, channel or chat.
    pub conversation: Option<MentionedConversation>,
}

/// A team, a channel or a chat that is mentioned as a whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MentionedConversation {
    pub id: Option<String>,
    pub display_name: Option<String>,
    /// `team`, `channel` or `chat`.
    pub conversation_identity_type: Option<String>,
}

/// A reaction to a message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Reaction {
    /// `like`, `heart`, `laugh`, `surprised`, `sad`, `angry`, or an emoji.
    pub reaction_type: Option<String>,
    pub created_date_time: Option<String>,
    pub user: Option<MessageSender>,
}

/// A message to send to a channel or a chat.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessage {
    /// What to say: `{ "contentType": "text" or "html", "content": "…" }`.
    pub body: ItemBody,
    /// A title for a new message in a channel. A reply and a chat message take none.
    pub subject: Option<String>,
    /// `normal`, `high` or `urgent`.
    pub importance: Option<String>,
    /// Who the body's `<at id="…">` tags are of. Needed for a mention to notify anyone.
    pub mentions: Option<Vec<Mention>>,
}

impl SendChatMessage {
    /// A plain text message.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            body: ItemBody::text(content),
            ..Self::default()
        }
    }

    /// A message written in HTML.
    pub fn html(content: impl Into<String>) -> Self {
        Self {
            body: ItemBody::html(content),
            ..Self::default()
        }
    }
}

impl ChatMessage {
    /// The body as plain text.
    ///
    /// A body that is already text is returned as it is. From HTML, each
    /// mention becomes `@` and the name, each attachment `[attachment: name]`
    /// on a line of its own, a picture or an emoji what it stands for, and a
    /// link its words with its address after them. Paragraphs and breaks
    /// become lines; every other tag is dropped and its text kept.
    pub fn plain_text(&self) -> String {
        let Some(body) = &self.body else {
            return String::new();
        };
        let content = body.content.as_deref().unwrap_or_default();
        if body
            .content_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("html"))
        {
            html_text(content, &self.attachments)
        } else {
            content.to_owned()
        }
    }

    /// The message with `text` filled in from its body.
    pub(crate) fn rendered(mut self) -> Self {
        self.text = self.plain_text();
        self
    }
}

/// Tags that end a line where they open or close.
const BLOCKS: [&str; 16] = [
    "p",
    "div",
    "li",
    "ul",
    "ol",
    "tr",
    "table",
    "blockquote",
    "pre",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
];

fn html_text(html: &str, attachments: &[ChatAttachment]) -> String {
    let mut out = String::new();
    // An open link: its address, and where its words begin in `out`.
    let mut link: Option<(String, usize)> = None;
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(&unescaped(&rest[..open]));
        let after = &rest[open + 1..];
        // A `<` that opens no tag is something that was said. A tag holds no
        // `<`, so the search for its end stops at the next one; looking
        // further would read the rest of the message once for every `<` in it.
        let close = after
            .find(['<', '>'])
            .filter(|&at| after[at..].starts_with('>') && is_tag(&after[..at]));
        let Some(close) = close else {
            out.push('<');
            rest = after;
            continue;
        };
        let tag = &after[..close];
        rest = &after[close + 1..];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match name.as_str() {
            // What a script or a style holds is not what was said.
            "script" | "style" if !closing => rest = after_closing(rest, &name),
            "br" => out.push('\n'),
            "at" if !closing => out.push('@'),
            "attachment" if !closing => {
                let id = attribute(tag, "id");
                let name = attachments
                    .iter()
                    .find(|attachment| attachment.id.is_some() && attachment.id == id)
                    .and_then(|attachment| attachment.name.as_deref())
                    .filter(|name| !name.trim().is_empty());
                match name {
                    Some(name) => out.push_str(&format!("\n[attachment: {name}]\n")),
                    None => out.push_str("\n[attachment]\n"),
                }
            }
            "img" | "emoji" => out.push_str(&attribute(tag, "alt").unwrap_or_default()),
            "a" if !closing => link = attribute(tag, "href").map(|address| (address, out.len())),
            "a" => {
                if let Some((address, from)) = link.take() {
                    let words = out.get(from..).unwrap_or_default().trim();
                    if !address.trim().is_empty() && words != address.trim() {
                        out.push_str(&format!(" ({})", address.trim()));
                    }
                }
            }
            name if BLOCKS.contains(&name) => out.push('\n'),
            _ => {}
        }
    }
    out.push_str(&unescaped(rest));
    // One space between words, no empty lines.
    out.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// What follows the tag that closes `name`, or nothing when it is never closed.
fn after_closing<'a>(rest: &'a str, name: &str) -> &'a str {
    let closes = |at: usize| {
        rest[at + 2..]
            .get(..name.len())
            .is_some_and(|found| found.eq_ignore_ascii_case(name))
    };
    rest.match_indices("</")
        .find(|&(at, _)| closes(at))
        .and_then(|(at, _)| rest[at..].find('>').map(|end| &rest[at + end + 1..]))
        .unwrap_or("")
}

/// True for what HTML puts between `<` and `>`: a name, with or without a
/// leading `/`, or a comment. Anything else was said, not marked up.
fn is_tag(inner: &str) -> bool {
    let name = inner.strip_prefix('/').unwrap_or(inner);
    name.starts_with(|c: char| c.is_ascii_alphabetic()) || inner.starts_with('!')
}

/// The value of one attribute of a tag, with its escapes read.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(&format!("{name}=")) {
        let at = from + found;
        let value = &tag[at + name.len() + 1..];
        // The name has to begin a word: `id=` and not the end of `data-id=`.
        let begins = at > 0 && tag[..at].ends_with(char::is_whitespace);
        if let (true, Some(quote)) = (begins, value.chars().next().filter(|c| matches!(c, '"' | '\''))) {
            let inner = &value[1..];
            return inner.find(quote).map(|end| unescaped(&inner[..end]));
        }
        from = at + name.len() + 1;
    }
    None
}

/// Text with HTML's escapes read. What looks like one and is not is kept as it was written.
fn unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let read = after.find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &after[..end];
            let character = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => {
                    let code = match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => name.strip_prefix('#').and_then(|decimal| decimal.parse().ok()),
                    };
                    code.and_then(char::from_u32)
                }
            };
            character.map(|character| (character, end))
        });
        match read {
            Some((character, end)) => {
                out.push(character);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
