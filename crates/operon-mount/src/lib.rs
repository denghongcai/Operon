#[cfg(any(target_os = "linux", target_os = "macos"))]
mod errors;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod fuse_attr;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod fuse_fs;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod inode_table;
pub mod mount_core;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod path;
pub mod remote_client;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod session;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_file_info;
#[cfg(windows)]
mod windows_path;
#[cfg(windows)]
mod windows_security;
#[cfg(windows)]
mod windows_status;

pub use mount_core::{MountAdapterCore, MountDirectoryEntry, MountErrorKind, RemoteFs};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use session::{spawn_mount, MountOptions, MountSession};
#[cfg(windows)]
pub use windows::{spawn_mount, MountOptions, MountSession};

pub const MOUNT_CAPABILITY: &str = "mount";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_core_api_is_available_from_crate_root() {
        fn accepts_remote_fs<T: mount_core::RemoteFs>() {}

        struct DummyRemoteFs;

        impl mount_core::RemoteFs for DummyRemoteFs {
            fn stat(&self, _path: &str) -> anyhow::Result<operon_core::FsStat> {
                unimplemented!("compile-only trait shape check")
            }

            fn list(&self, _path: &str) -> anyhow::Result<operon_core::FsList> {
                unimplemented!("compile-only trait shape check")
            }

            fn read_range(&self, _path: &str, _offset: u64, _size: u32) -> anyhow::Result<Vec<u8>> {
                unimplemented!("compile-only trait shape check")
            }

            fn write_range(&self, _path: &str, _offset: u64, _data: &[u8]) -> anyhow::Result<u64> {
                unimplemented!("compile-only trait shape check")
            }

            fn truncate(&self, _path: &str, _size: u64) -> anyhow::Result<operon_core::FsStat> {
                unimplemented!("compile-only trait shape check")
            }

            fn mkdir(&self, _path: &str) -> anyhow::Result<operon_core::FsStat> {
                unimplemented!("compile-only trait shape check")
            }

            fn delete(&self, _path: &str) -> anyhow::Result<()> {
                unimplemented!("compile-only trait shape check")
            }

            fn rename(&self, _from_path: &str, _to_path: &str) -> anyhow::Result<()> {
                unimplemented!("compile-only trait shape check")
            }
        }

        accepts_remote_fs::<DummyRemoteFs>();
        assert_eq!(
            mount_core::normalize_remote_path("/workspace//project/").unwrap(),
            "/workspace/project"
        );
        assert_eq!(
            mount_core::join_remote_child("/workspace", "file.txt").unwrap(),
            "/workspace/file.txt"
        );
    }
}
