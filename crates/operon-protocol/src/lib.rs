//! Generated gRPC types and the transport/domain conversion boundary.
//!
//! Conversion implementations live outside the crate entrypoint so protocol
//! generation, versioning, and mapping code can evolve independently.

pub const PROTOCOL_VERSION: &str = "v0.16.9";

mod conversions;

pub use conversions::*;
