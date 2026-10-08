//! Generated gRPC types and the transport/domain conversion boundary.
//!
//! Conversion implementations live outside the crate entrypoint so protocol
//! generation, versioning, and mapping code can evolve independently.

pub const PROTOCOL_VERSION: &str = "v0.16.11";
pub const MAX_FS_DATA_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_GRPC_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub const KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
pub const KEEPALIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

mod conversions;

pub use conversions::*;
