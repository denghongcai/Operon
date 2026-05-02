use std::{fs, path::Path};

#[cfg(unix)]
use std::{
    fs::OpenOptions,
    io::Write as _,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};

#[cfg(not(unix))]
use std::io::Write as _;

pub(crate) fn generate_token() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)?;
    let mut token = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        token.push(HEX[(byte >> 4) as usize] as char);
        token.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(token)
}

#[cfg(unix)]
pub(crate) fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    validate_private_file_target(path)?;
    let mut handle = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    handle.write_all(content.as_bytes())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    let mut handle = fs::File::create(path)?;
    handle.write_all(content.as_bytes())?;
    Ok(())
}

#[cfg(unix)]
fn validate_private_file_target(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        !metadata.file_type().is_symlink(),
        "refusing to write private file {} because it is a symlink",
        path.display()
    );
    let mode = metadata.permissions().mode() & 0o777;
    anyhow::ensure!(
        mode & 0o077 == 0,
        "refusing to write private file {} with permissions {:03o}; set permissions to 600 first",
        path.display(),
        mode
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn generated_token_is_hex_encoded() {
        let token = generate_token().expect("token");
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|value| value.is_ascii_hexdigit()));
    }

    #[cfg(unix)]
    #[test]
    fn private_file_refuses_broad_existing_permissions() {
        let base = unique_temp_dir("operon-private-file-test");
        let path = base.join("token");
        fs::write(&path, "old\n").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

        let error =
            write_private_file(&path, "new\n").expect_err("broad token file should be rejected");

        assert!(error.to_string().contains("refusing to write private file"));
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn private_file_is_written_with_owner_only_permissions() {
        let base = unique_temp_dir("operon-private-file-write-test");
        let path = base.join("token");

        write_private_file(&path, "new\n").expect("write private file");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(base);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }
}
