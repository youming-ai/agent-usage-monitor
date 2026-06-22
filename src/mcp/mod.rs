//! MCP (Model Context Protocol) server for agent usage queries.
//!
//! Reuses `stats::collect()` to drive all tool/resource handlers. Each
//! call triggers a fresh scan — no caching.

pub mod server;
pub mod types;
pub use server::AumMcpServer;
