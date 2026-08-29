//! `forma-server`: the single authority for documents, validation, rendering and
//! persistence (plan sections 3.2 and 5). React and `forma-mcp` are both clients.

pub mod auth;
pub mod backup;
pub mod config;
pub mod error;
pub mod routes;
pub mod state;
pub mod telemetry;
pub mod test_card;

pub use config::Config;
pub use state::AppState;
