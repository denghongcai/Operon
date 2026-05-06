#![cfg(windows)]

use operon_core::FsStat;
use winfsp_wrs::FileAttributes;
use winfsp_wrs_sys::FSP_FSCTL_FILE_INFO;

pub(super) fn attributes_for_stat(stat: &FsStat) -> FileAttributes {
    if stat.is_dir {
        FileAttributes::DIRECTORY
    } else {
        FileAttributes::NORMAL
    }
}

pub(super) fn file_info_for_stat(stat: &FsStat) -> FSP_FSCTL_FILE_INFO {
    let mut info = FSP_FSCTL_FILE_INFO::default();
    let size = if stat.is_dir { 0 } else { stat.size };
    let now = winfsp_wrs::filetime_now();
    info.FileAttributes = attributes_for_stat(stat).0;
    info.AllocationSize = size.div_ceil(4096) * 4096;
    info.FileSize = size;
    info.CreationTime = now;
    info.LastAccessTime = now;
    info.LastWriteTime = now;
    info.ChangeTime = now;
    info.HardLinks = 1;
    info
}
