//! MCP protocol and stdio adapter for the shared Knowledge-Brain application layer.

mod protocol;
mod server;
mod wire;

pub use protocol::MODERN_PROTOCOL_VERSION;
pub use server::{LATEST_LEGACY_PROTOCOL_VERSION, LEGACY_PROTOCOL_VERSIONS, McpServer};
pub use wire::serve_frames;
