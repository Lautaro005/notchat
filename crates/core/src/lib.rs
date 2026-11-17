//! Notchat core: WhatsApp session, encrypted storage, AI providers, the
//! assistant agent and auto-reply policy. No UI or Tauri dependencies.

pub mod agent;
pub mod ai;
pub mod attach;
pub mod error;
pub mod keychain;
pub mod managed;
pub mod net;
pub mod policy;
pub mod store;
pub mod wa;

pub use error::{Error, Result};
