//! MCP stdio adapter for the shared Knowledge-Brain application layer.

mod server;
mod wire;

pub use server::McpServer;
pub use wire::serve_frames;
