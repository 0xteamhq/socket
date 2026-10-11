//! Messages in a channel or a chat: what Graph returns for them, and the content used to send one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;
use super::{ItemBody, html};

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
    /// Socket writes this from `body`; anything Graph sent here is not used.
    #[serde(deserialize_with = "nullable")]
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
    /// link its words with its address after them, and the cells of a table
    /// are kept apart. Paragraphs and breaks become lines; every other tag is
    /// dropped and its text kept.
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
            html::text(content, |id| {
                self.attachments
                    .iter()
                    .find(|attachment| attachment.id.as_deref() == Some(id))
                    .and_then(|attachment| attachment.name.clone())
            })
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
