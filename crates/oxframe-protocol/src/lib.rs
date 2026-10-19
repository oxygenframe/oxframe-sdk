//! Protocol shared by the engine (`oxframe-preview`), the CLI and `editor-ui`.
//!
//! This crate is the single source of truth: TypeScript types for
//! `@oxframe/protocol` are generated from it with `ts-rs`.

#![warn(missing_docs)]

mod messages;
mod meta;

pub use messages::*;
pub use meta::*;

/// Incremented on every incompatible change of [`Command`], [`Event`] or the
/// binary frame header. Every side compares it in `Hello`/`Ready`.
pub const PROTOCOL_VERSION: u32 = 1;
