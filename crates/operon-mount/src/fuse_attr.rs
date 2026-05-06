#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::time::UNIX_EPOCH;

use crate::inode_table::InodeEntry;

const STAT_BLOCK_SIZE: u32 = 512;

pub(crate) fn attr_owner() -> (u32, u32) {
    #[cfg(target_os = "macos")]
    {
        // FUSE-T's NFS bridge behaves more like a user-mounted filesystem than
        // the Linux kernel FUSE path, so report files as owned by the mounter.
        return unsafe { (libc::getuid(), libc::getgid()) };
    }

    #[cfg(not(target_os = "macos"))]
    {
        (0, 0)
    }
}

pub(crate) fn file_attr(entry: &InodeEntry) -> fuser::FileAttr {
    let (uid, gid) = attr_owner();
    fuser::FileAttr {
        ino: entry.ino,
        size: if entry.is_dir { 0 } else { entry.size },
        blocks: if entry.is_dir {
            0
        } else {
            entry.size.div_ceil(STAT_BLOCK_SIZE as u64)
        },
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: if entry.is_dir {
            fuser::FileType::Directory
        } else {
            fuser::FileType::RegularFile
        },
        perm: if entry.is_dir { 0o755 } else { 0o644 },
        nlink: if entry.is_dir { 2 } else { 1 },
        uid,
        gid,
        rdev: 0,
        flags: 0,
        blksize: STAT_BLOCK_SIZE,
    }
}

pub(crate) fn attr_trace_detail(attr: &fuser::FileAttr) -> String {
    format!(
        "ino={:?} kind={:?} size={} blocks={} blksize={} perm={:o} nlink={} uid={} gid={}",
        attr.ino,
        attr.kind,
        attr.size,
        attr.blocks,
        attr.blksize,
        attr.perm,
        attr.nlink,
        attr.uid,
        attr.gid
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_directories_with_directory_permissions() {
        let entry = InodeEntry {
            ino: fuser::INodeNo::ROOT,
            parent: fuser::INodeNo::ROOT,
            name: String::new(),
            path: "/".to_string(),
            is_dir: true,
            size: 0,
        };

        let attr = file_attr(&entry);

        assert_eq!(attr.kind, fuser::FileType::Directory);
        assert_eq!(attr.perm, 0o755);
        assert_eq!(attr.nlink, 2);
        assert_eq!(attr.size, 0);
        assert_eq!(attr.blocks, 0);
    }

    #[test]
    fn uses_fuse_compatible_512_byte_stat_blocks() {
        let entry = InodeEntry {
            ino: fuser::INodeNo(2),
            parent: fuser::INodeNo::ROOT,
            name: "file.txt".to_string(),
            path: "/file.txt".to_string(),
            is_dir: false,
            size: 513,
        };

        let attr = file_attr(&entry);

        assert_eq!(attr.blksize, 512);
        assert_eq!(attr.blocks, 2);
    }

    #[test]
    fn reports_platform_owner() {
        let entry = InodeEntry {
            ino: fuser::INodeNo::ROOT,
            parent: fuser::INodeNo::ROOT,
            name: String::new(),
            path: "/".to_string(),
            is_dir: true,
            size: 0,
        };

        let attr = file_attr(&entry);
        let (expected_uid, expected_gid) = attr_owner();

        assert_eq!(attr.uid, expected_uid);
        assert_eq!(attr.gid, expected_gid);
    }
}
