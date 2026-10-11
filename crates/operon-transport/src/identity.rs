//! Private, restart-stable embedded endpoint identity, with create-only
//! publication. Existing invalid/broad/symlink files are never overwritten.

use iroh::SecretKey;
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};
use zeroize::Zeroizing;

pub fn load_or_create_identity(path: &Path) -> io::Result<SecretKey> {
    match fs::symlink_metadata(path) {
        Ok(_) => return load_identity(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    operon_config::protect_new_private_file(staged.path()).map_err(io::Error::other)?;
    let mut bytes = Zeroizing::new([0u8; 32]);
    getrandom::fill(&mut *bytes).map_err(io::Error::other)?;
    staged.write_all(&*bytes)?;
    staged.as_file().sync_all()?;
    match staged.persist_noclobber(path) {
        Ok(file) => {
            drop(file);
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
        }
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            // A concurrent initializer won. Its published identity is the
            // authority; never replace it or return our unpublished key.
            drop(error.file);
        }
        Err(error) => return Err(error.error),
    }
    load_identity(path)
}

pub fn load_identity(path: &Path) -> io::Result<SecretKey> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "iroh identity must be a regular non-symlink file",
        ));
    }
    operon_config::validate_private_file_permissions(path).map_err(io::Error::other)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc_no_follow());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(path)?;
    operon_config::validate_private_file_handle_permissions(&file).map_err(io::Error::other)?;
    let opened = file.metadata()?;
    if !opened.is_file() || opened.len() != 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "iroh identity must contain exactly 32 private key bytes",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if opened.permissions().mode() & 0o077 != 0
            || opened.dev() != metadata.dev()
            || opened.ino() != metadata.ino()
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "iroh identity changed or has broad permissions",
            ));
        }
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    file.read_exact(&mut *bytes)?;
    let mut extra = [0u8; 1];
    if file.read(&mut extra)? != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "iroh identity changed length during read",
        ));
    }
    Ok(SecretKey::from_bytes(&bytes))
}

#[cfg(unix)]
fn libc_no_follow() -> i32 {
    // Values vary across Unix targets; use the platform libc constant.
    libc::O_NOFOLLOW
}
