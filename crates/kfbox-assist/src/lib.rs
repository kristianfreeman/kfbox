//! kfbox's LLM features: quick functions, where a short instruction does the
//! work with kfbox's own commands as the model's tools, and the rules wizard,
//! where the model writes a rules file from your habits and your hand cues.
//!
//! The API key stays in the macOS Keychain ([`keys`]). Only this crate reads
//! it, to put it on a request; it never goes to the page, to `kfbox.db`, to a
//! settings file, to a log or into the library's history. Writes the model
//! makes go through the same safe path as any other, and undo the same way.

pub mod client;
pub mod keys;
pub mod quick;
pub mod settings;
pub mod wizard;

pub use client::{AssistError, HttpTransport, Transport};
pub use keys::{KeyStore, Keychain, MemoryKeys};
pub use settings::{Model, Settings};
