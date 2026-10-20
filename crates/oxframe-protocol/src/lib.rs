//! Protocol shared by the engine (`oxframe-preview`), the CLI and `editor-ui`.
//!
//! > **Status:** oxframe is under very active development. The API is not stable yet and
//! > may change from version to version. Pin the exact version in your Cargo.toml and check
//! > the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md)
//! > before upgrading.
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
