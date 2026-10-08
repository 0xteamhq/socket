//! Builds a `Socket` with every integration and prints what it can do.
//! No network and no credentials.
//!
//! Run with `cargo run -p socketkit --all-features --example catalogue`.

use std::sync::Arc;

use socketkit::{AuthScheme, MemoryTokenStore, Result, Socket};

fn main() -> Result<()> {
    let socket = Socket::builder(Arc::new(MemoryTokenStore::new()))
        .integration(Arc::new(socketkit::github::GitHub::new()))
        .integration(Arc::new(socketkit::google::Google::new()))
        .integration(Arc::new(socketkit::linear::Linear::new()))
        .integration(Arc::new(socketkit::notion::Notion::new()))
        .integration(Arc::new(socketkit::slack::Slack::new()))
        .integration(Arc::new(socketkit::zoom::Zoom::new()))
        .build()?;

    for provider in socket.providers() {
        let auth = match &provider.auth {
            AuthScheme::OAuth2(oauth) => format!("OAuth 2.0, default scopes: {}", oauth.default_scopes.join(" ")),
            AuthScheme::ApiKey(_) => "API key".to_owned(),
        };
        println!("{} ({}) at {}", provider.display_name, provider.id, provider.api_base);
        println!("  {auth}");
    }
    println!();
    for operation in socket.operations() {
        println!("{:<28} {:?}", operation.name, operation.effect);
    }
    Ok(())
}
