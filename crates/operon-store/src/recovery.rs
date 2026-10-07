use std::{
    io::{Read, Write},
    path::Path,
};

use anyhow::Context;
use serde_json::Value;

struct Parsed {
    records: Vec<Value>,
    repair: bool,
    lost_tail_bytes: usize,
}

fn parse(content: &[u8]) -> anyhow::Result<Parsed> {
    let mut stream = serde_json::Deserializer::from_slice(content).into_iter::<Value>();
    let mut records = Vec::new();
    let mut lost_tail_bytes = 0;
    while let Some(record) = stream.next() {
        match record {
            Ok(value) if value.is_object() => records.push(value),
            Ok(_) => anyhow::bail!("store record is not a JSON object"),
            Err(error) => {
                let tail = &content[stream.byte_offset()..];
                // Only an unterminated final compact record can be discarded.
                // A malformed interior record or a newline-terminated error is fatal.
                let tail = tail.strip_prefix(b"\n").unwrap_or(tail);
                let tail = tail
                    .iter()
                    .copied()
                    .skip_while(u8::is_ascii_whitespace)
                    .collect::<Vec<_>>();
                if error.is_eof() && tail.starts_with(b"{") && !tail.contains(&b'\n') {
                    lost_tail_bytes = tail.len();
                    break;
                }
                return Err(error).context("unsupported store corruption; original file preserved");
            }
        }
    }
    let repair = lost_tail_bytes > 0
        || (!content.is_empty() && !content.ends_with(b"\n"))
        || content
            .split(|b| *b == b'\n')
            .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
            .any(|line| serde_json::from_slice::<Value>(line).is_err());
    Ok(Parsed {
        records,
        repair,
        lost_tail_bytes,
    })
}

fn read_source(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "store path is not a regular file"
    );
    let mut content = Vec::new();
    file.read_to_end(&mut content)?;
    Ok(Some(content))
}

pub(crate) fn load_records(path: &Path) -> anyhow::Result<Vec<Value>> {
    let Some(content) = read_source(path)? else {
        return Ok(Vec::new());
    };
    let parsed = parse(&content)?;
    if parsed.repair {
        tracing::warn!(path = %path.display(), lost_tail_bytes = parsed.lost_tail_bytes,
            "reading recoverable store records; original source unchanged");
    }
    Ok(parsed.records)
}

/// Normalize only unambiguously recoverable JSON records, preserving a private
/// byte-for-byte backup before replacing the source. Unsupported corruption fails.
pub fn recover_store(path: Option<&Path>) -> anyhow::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    let _guard = super::APPEND_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("store append lock poisoned"))?;
    recover_store_unlocked(path)
}

pub(crate) fn recover_store_unlocked(path: &Path) -> anyhow::Result<()> {
    let Some(content) = read_source(path)? else {
        return Ok(());
    };
    let parsed = parse(&content)?;
    if !parsed.repair {
        return Ok(());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut backup = tempfile::Builder::new()
        .prefix(".operon-store-recovery-")
        .tempfile_in(parent)?;
    backup.write_all(&content)?;
    backup.as_file().sync_all()?;
    let (_, backup_path) = backup.keep()?;
    let mut replacement = tempfile::NamedTempFile::new_in(parent)?;
    for record in parsed.records {
        serde_json::to_writer(replacement.as_file_mut(), &record)?;
        replacement.write_all(b"\n")?;
    }
    replacement.as_file().sync_all()?;
    replacement.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    tracing::warn!(path = %path.display(), backup = %backup_path.display(),
        lost_tail_bytes = parsed.lost_tail_bytes, "recovered store; original bytes preserved in backup");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_preserves_concatenated_objects_unicode_and_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store");
        let original =
            "{\"kind\":\"audit\",\"text\":\"中文 \\\"{}\\\"\"}{\"kind\":\"exec\"}\n\n{\"kind\":";
        std::fs::write(&path, original).unwrap();
        recover_store(Some(&path)).unwrap();
        assert_eq!(load_records(&path).unwrap().len(), 2);
        let backup = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".operon-store-recovery-")
            })
            .unwrap();
        assert_eq!(std::fs::read_to_string(backup).unwrap(), original);
        for line in std::fs::read_to_string(path).unwrap().lines() {
            assert!(serde_json::from_str::<Value>(line).unwrap().is_object());
        }
    }

    #[test]
    fn rejects_interior_corruption_without_modifying_source() {
        for content in [
            "{}\nBROKEN\n{}\n",
            "{}\n{\"x\":\n",
            "{}\n{broken}",
            "{}\n42\n",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("store");
            std::fs::write(&path, content).unwrap();
            assert!(recover_store(Some(&path)).is_err(), "{content}");
            assert_eq!(std::fs::read_to_string(path).unwrap(), content);
        }
    }

    #[test]
    fn missing_newline_is_normalized_before_append() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store");
        std::fs::write(&path, "{}").unwrap();
        super::super::append_record(Some(&path), &serde_json::json!({"next": true})).unwrap();
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "{}\n{\"next\":true}\n"
        );
    }
}
