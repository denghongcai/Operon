//! Owned byte-stream connection backends, independent of capability semantics.
//!
//! Enabling a build feature does not enable a listener, relay or discovery.
//! Applications choose and configure their backend explicitly.

#[cfg(feature = "iroh")]
pub mod embedded;

#[cfg(feature = "iroh")]
pub mod identity;
