//! Generated gRPC types and the transport/domain conversion boundary.
//!
//! Conversion implementations live outside the crate entrypoint so protocol
//! generation, versioning, and mapping code can evolve independently.

mod conversions;

pub use conversions::*;
