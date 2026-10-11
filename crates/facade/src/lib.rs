//! Socket: connection, authorisation and operation layers for external services.
//!
//! This crate contains no logic. It re-exports the core and, behind one
//! feature per vendor, the integration crates.

pub use socketkit_core::*;

#[cfg(feature = "attio")]
pub use socketkit_attio as attio;
#[cfg(feature = "github")]
pub use socketkit_github as github;
#[cfg(feature = "google")]
pub use socketkit_google as google;
#[cfg(feature = "hubspot")]
pub use socketkit_hubspot as hubspot;
#[cfg(feature = "linear")]
pub use socketkit_linear as linear;
#[cfg(feature = "microsoft")]
pub use socketkit_microsoft as microsoft;
#[cfg(feature = "notion")]
pub use socketkit_notion as notion;
#[cfg(feature = "slack")]
pub use socketkit_slack as slack;
#[cfg(feature = "zoom")]
pub use socketkit_zoom as zoom;
