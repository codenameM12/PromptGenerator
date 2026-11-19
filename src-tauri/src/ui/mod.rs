//! Rust view-model layer for the native UI port.
//!
//! Everything in this module is driven by the recorded corpus in `port/corpus/`
//! and compared against the TypeScript source it replaces. See
//! `PARITY-CONTRACT.md` for the rules this layer must obey.

pub mod crop_parity;
pub mod edit_history;
pub mod errors;
pub mod preset_template;
pub mod status;
pub mod workspace_draft;
