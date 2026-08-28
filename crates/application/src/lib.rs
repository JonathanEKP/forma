//! Use cases (plan section 8.2).
//!
//! This crate knows nothing about Actix or MCP: it takes repository traits and
//! returns domain types, so the REST server and the MCP process share one
//! implementation of every operation rather than two that drift apart.

pub mod asset_requests;
pub mod assets;
pub mod designs;
pub mod error;
pub mod exports;
pub mod projects;
pub mod renders;

pub use error::{ApplicationError, Result};
