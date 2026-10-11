//! The data Google returns, and the content and options its methods take.
//!
//! One file per area of an API. Everything is re-exported here, so a caller
//! writes `socketkit::google::models::GmailMessage` whichever file it lives in.
//!
//! The types follow Google's own shapes and are written in JSON with Google's
//! own names (`threadId`, `hangoutLink`), so what Google's documentation says
//! about a field holds here too. Every field Google may omit has a default,
//! so a response that carries less than these types describe still reads.

mod paging;

pub use paging::Paging;

// ── gmail: modules ──

// ── calendar: modules ──

// ── meet: modules ──

// ── drive: modules ──

// ── docs and sheets: modules ──

// ── gmail: types ──

// ── calendar: types ──

// ── meet: types ──

// ── drive: types ──

// ── docs and sheets: types ──
