#![cfg(target_os = "linux")]

mod errors;
mod fuse_fs;
mod inode_table;
mod path;
mod remote_client;
mod session;

pub use remote_client::RemoteFs;
pub use session::{spawn_mount, MountOptions, MountSession};

pub const MOUNT_CAPABILITY: &str = "mount";
