//! Optional in-process Node binding, independent of capability orchestration.
//! Ordinary workspace builds do not load or link a Node runtime.

#[cfg(feature = "node")]
mod node;

#[cfg(feature = "node")]
pub use node::binding_version;
