//! A tour of the Slack integration against a real workspace.
//!
//! ```sh
//! # List every Slack operation. Needs no token.
//! cargo run -p socketkit --features slack --example slack -- operations
//!
//! # Read-only tour: who am I, the workspace, channels, recent messages, members.
//! SLACK_TOKEN=xoxb-… SLACK_CHANNEL=C0123ABCD cargo run -p socketkit --features slack --example slack
//!
//! # The same, then post, react, reply, edit, pin, schedule and clean up after itself.
//! SLACK_TOKEN=xoxb-… SLACK_CHANNEL=C0123ABCD cargo run -p socketkit --features slack --example slack -- write
//! ```
//!
//! `SLACK_TOKEN` is a bot token (`xoxb-…`) or a user token (`xoxp-…`).
//! `SLACK_CHANNEL` is the id of a channel the token's bot or user is a member of.
//! The read-only tour needs the scopes `channels:read`, `channels:history`,
//! `users:read` and `team:read`. The write tour also needs `chat:write`,
//! `reactions:write` and `pins:write`.
//!
//! Reading these two environment variables is this example's choice. The
//! library itself never reads the environment.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::json;
use socketkit::slack::Slack;
use socketkit::slack::models::{History, ListConversations, Paging, PostMessage, UpdateMessage};
use socketkit::{Connection, ConnectionKey, Effect, Error, ErrorKind, Integration, ProviderId, Result, Retry, Socket};

#[tokio::main]
async fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode == "operations" {
        return print_operations();
    }
    let (Ok(token), Ok(channel)) = (std::env::var("SLACK_TOKEN"), std::env::var("SLACK_CHANNEL")) else {
        eprintln!("Set SLACK_TOKEN and SLACK_CHANNEL, or run with `operations`. See the top of this file.");
        std::process::exit(2);
    };
    if let Err(error) = tour(&token, &channel, mode == "write").await {
        explain(&error);
        std::process::exit(1);
    }
}

/// Every operation, as a table: what an agent or another language sees.
fn print_operations() {
    println!("| Operation | Effect | Scope | What it does |");
    println!("| --- | --- | --- | --- |");
    for operation in Slack::new().operations() {
        let effect = match operation.effect {
            Effect::Read => "read",
            Effect::Write => "write",
            Effect::Destructive => "destructive",
        };
        let scopes = operation.required_scopes.join(", ");
        println!(
            "| `{}` | {effect} | {scopes} | {} |",
            operation.name, operation.description
        );
    }
}

async fn tour(token: &str, channel: &str, write: bool) -> Result<()> {
    // 1. Give Slack its connection details, and build one Socket.
    //    `Socket::in_memory()` is enough here because the token is fixed.
    let slack = Slack::with_token(token);
    let socket = Socket::in_memory().integration(Arc::new(slack.clone())).build()?;

    // 2. A connection is one provider for one tenant. With a fixed token the
    //    tenant is only a label.
    let key = ConnectionKey::new(ProviderId::new("slack")?, "example");
    let connection = socket.connection(key.clone()).await?;

    // 3. Typed methods, grouped by area. Identifiers are plain arguments.
    let me = slack.identity(&connection).await?;
    println!("Signed in as {} ({})", me.name, me.id);

    let team = slack.workspace(&connection).info().await?;
    println!("Workspace: {} ({}.slack.com)", team.name, team.domain);

    // Filters and paging are structs; leave a field unset to get Slack's default.
    let options = ListConversations {
        limit: Some(5),
        exclude_archived: Some(true),
        ..Default::default()
    };
    let channels = slack.conversations(&connection).list(options).await?;
    for found in &channels.items {
        println!(
            "  #{} ({})",
            found.name.as_deref().unwrap_or("direct message"),
            found.id
        );
    }
    // Lists come a page at a time. Pass `next_cursor` back to get the next page.
    if let Some(cursor) = channels.next_cursor {
        let next = ListConversations {
            limit: Some(5),
            cursor: Some(cursor),
            ..Default::default()
        };
        let more = slack.conversations(&connection).list(next).await?;
        println!("  … and {} more on the next page", more.items.len());
    }

    let info = slack.conversations(&connection).info(channel).await?;
    println!(
        "Channel {}: {} members",
        info.name.as_deref().unwrap_or(channel),
        info.num_members.unwrap_or(0)
    );

    let history = slack
        .conversations(&connection)
        .history(
            channel,
            History {
                limit: Some(3),
                ..Default::default()
            },
        )
        .await?;
    for message in &history.items {
        println!("  [{}] {}", message.ts, message.text.lines().next().unwrap_or(""));
    }

    let members = slack
        .users(&connection)
        .list(Paging {
            limit: Some(3),
            ..Default::default()
        })
        .await?;
    for member in &members.items {
        println!("  @{} {}", member.name, member.real_name.as_deref().unwrap_or(""));
    }

    // 4. The same operations by name with JSON: what an agent calls.
    let by_name = socket
        .invoke(key, "slack.conversations.info".into(), json!({ "channel": channel }))
        .await?;
    println!("By name: {}", by_name["name"]);

    if write {
        write_tour(&slack, &connection, channel).await?;
    } else {
        println!("Run with `write` to post, react, reply, edit, pin and schedule.");
    }
    Ok(())
}

/// Posts a message, works on it, and removes everything it created.
async fn write_tour(slack: &Slack, connection: &Connection, channel: &str) -> Result<()> {
    let chat = slack.chat(connection);

    // Content is a struct: text, blocks or attachments, and where it goes.
    let posted = chat
        .post_message(channel, PostMessage::text("Hello from Socket"))
        .await?;
    println!("Posted {}", posted.ts);

    slack.reactions(connection).add(channel, &posted.ts, "wave").await?;
    chat.post_message(
        channel,
        PostMessage::text("A reply in the thread").in_thread(&posted.ts),
    )
    .await?;
    chat.update(channel, &posted.ts, UpdateMessage::text("Hello from Socket (edited)"))
        .await?;
    println!("Link: {}", chat.permalink(channel, &posted.ts).await?);

    slack.pins(connection).add(channel, &posted.ts).await?;
    println!("Pinned items: {}", slack.pins(connection).list(channel).await?.len());
    slack.pins(connection).remove(channel, &posted.ts).await?;

    // Scheduling takes the time as seconds since the Unix epoch.
    let in_ten_minutes = SystemTime::now() + Duration::from_secs(600);
    let post_at = in_ten_minutes
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let scheduled = chat
        .schedule_message(channel, post_at, PostMessage::text("Scheduled by Socket"))
        .await?;
    chat.delete_scheduled_message(channel, &scheduled.id).await?;
    println!("Scheduled and cancelled {}", scheduled.id);

    // Clean up: the thread reply, then the message.
    let thread = slack
        .conversations(connection)
        .replies(channel, &posted.ts, History::default())
        .await?;
    for reply in thread.items.iter().rev() {
        chat.delete(channel, &reply.ts).await?;
    }
    println!("Removed what this example posted.");
    Ok(())
}

/// Every error says what kind it is, so a program can decide what to do.
fn explain(error: &Error) {
    let advice = match error.kind() {
        ErrorKind::ReconnectRequired => "The token is no longer accepted. Issue a new one.".to_owned(),
        ErrorKind::AccessDenied => "The token lacks a scope, or the bot is not in the channel.".to_owned(),
        ErrorKind::NotFound => "Slack has no such channel, user or message.".to_owned(),
        ErrorKind::InvalidInput => "An argument was wrong; the message says which.".to_owned(),
        ErrorKind::RateLimited => match error.retry() {
            Retry::After(wait) => format!("Slack is throttling. Try again in {} seconds.", wait.as_secs()),
            _ => "Slack is throttling. Try again later.".to_owned(),
        },
        _ => "Something else went wrong.".to_owned(),
    };
    eprintln!("{error}\n{advice}");
}
