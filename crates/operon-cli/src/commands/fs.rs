use std::{fs, path::PathBuf};

use operon_core::{FsList, FsRead, FsWrite};

use crate::{
    grpc,
    output::{print_json, OutputMode},
    target::{load_endpoint, parse_node_path},
};

#[derive(Debug, serde::Serialize)]
struct FsReadOutputSummary {
    path: String,
    output: String,
    bytes_written: u64,
}

pub(crate) async fn stat(
    config_path: PathBuf,
    target: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let stat = grpc::fs_stat(&endpoint, &target.path).await?;
    if output.json {
        print_json(&stat)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

    println!(
        "{}:{} file={} dir={} size={}",
        target.node_id, stat.path, stat.is_file, stat.is_dir, stat.size
    );

    Ok(())
}

pub(crate) async fn list(
    config_path: PathBuf,
    target: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let list: FsList = grpc::fs_list(&endpoint, &target.path).await?;
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

    for entry in list.entries {
        println!(
            "{}\t{}\t{}",
            if entry.is_dir { "dir" } else { "file" },
            entry.size,
            entry.path
        );
    }

    Ok(())
}

pub(crate) async fn read(
    config_path: PathBuf,
    target: &str,
    file_output: Option<PathBuf>,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;

    if let Some(file_output) = file_output {
        let mut file = fs::File::create(&file_output)?;
        grpc::read_file_to_writer(&endpoint, &target.path, &mut file).await?;
        let bytes_written = file.metadata()?.len();
        if output.json {
            print_json(&FsReadOutputSummary {
                path: target.path,
                output: file_output.display().to_string(),
                bytes_written,
            })?;
        }
    } else {
        let mut content = Vec::new();
        grpc::read_file_to_writer(&endpoint, &target.path, &mut content).await?;
        let read = FsRead {
            path: target.path.clone(),
            content: String::from_utf8(content)?,
        };
        if output.json {
            print_json(&read)?;
            return Ok(());
        }
        if output.quiet {
            return Ok(());
        }
        print!("{}", read.content);
    }

    Ok(())
}

pub(crate) async fn write(
    config_path: PathBuf,
    target: &str,
    content: Option<String>,
    file: Option<PathBuf>,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;

    let write: FsWrite = match (content, file) {
        (Some(content), None) => {
            grpc::write_file_bytes(&endpoint, &target.path, content.as_bytes()).await?
        }
        (None, Some(file)) => grpc::write_file(&endpoint, &target.path, &file).await?,
        (Some(_), Some(_)) => anyhow::bail!("use either --content or --file, not both"),
        (None, None) => anyhow::bail!("fs write requires --content or --file"),
    };
    if output.json {
        print_json(&write)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

    println!(
        "{}:{} bytes_written={}",
        target.node_id, write.path, write.bytes_written
    );

    Ok(())
}

pub(crate) async fn mkdir(
    config_path: PathBuf,
    target: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let stat = grpc::fs_mkdir(&endpoint, &target.path).await?;
    if output.json {
        print_json(&stat)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{}:{} file={} dir={} size={}",
        target.node_id, stat.path, stat.is_file, stat.is_dir, stat.size
    );
    Ok(())
}

pub(crate) async fn rm(
    config_path: PathBuf,
    target: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let path = grpc::fs_delete(&endpoint, &target.path).await?;
    let result = serde_json::json!({ "path": path });
    if output.json {
        print_json(&result)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{}:{} deleted=true",
        target.node_id,
        result["path"].as_str().unwrap_or_default()
    );
    Ok(())
}

pub(crate) async fn rename(
    config_path: PathBuf,
    from: &str,
    to: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let from = parse_node_path(from)?;
    let to = parse_node_path(to)?;
    if from.node_id != to.node_id {
        anyhow::bail!("fs rename requires source and target to use the same node");
    }
    let endpoint = load_endpoint(config_path, &from.node_id)?;
    let (from_path, to_path) = grpc::fs_rename(&endpoint, &from.path, &to.path).await?;
    let result = serde_json::json!({
        "from_path": from_path,
        "to_path": to_path,
    });
    if output.json {
        print_json(&result)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{}:{} -> {}",
        from.node_id,
        result["from_path"].as_str().unwrap_or_default(),
        result["to_path"].as_str().unwrap_or_default()
    );
    Ok(())
}

pub(crate) async fn copy(
    config_path: PathBuf,
    from: &str,
    to: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let from = parse_node_path(from)?;
    let to = parse_node_path(to)?;
    if from.node_id != to.node_id {
        anyhow::bail!("fs copy requires source and target to use the same node");
    }
    let endpoint = load_endpoint(config_path, &from.node_id)?;
    let (from_path, to_path, bytes_copied) = grpc::fs_copy(&endpoint, &from.path, &to.path).await?;
    let result = serde_json::json!({
        "from_path": from_path,
        "to_path": to_path,
        "bytes_copied": bytes_copied,
    });
    if output.json {
        print_json(&result)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{}:{} -> {} bytes_copied={}",
        from.node_id,
        result["from_path"].as_str().unwrap_or_default(),
        result["to_path"].as_str().unwrap_or_default(),
        result["bytes_copied"].as_u64().unwrap_or_default()
    );
    Ok(())
}

pub(crate) async fn truncate(
    config_path: PathBuf,
    target: &str,
    size: u64,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let stat = grpc::fs_truncate(&endpoint, &target.path, size).await?;
    if output.json {
        print_json(&stat)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{}:{} file={} dir={} size={}",
        target.node_id, stat.path, stat.is_file, stat.is_dir, stat.size
    );
    Ok(())
}
