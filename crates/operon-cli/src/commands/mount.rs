use std::path::PathBuf;

use crate::output::OutputMode;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use crate::{
    output::print_json,
    target::{load_endpoint, parse_node_path},
};

pub(crate) fn mount_fs(
    config_path: PathBuf,
    target: &str,
    destination: PathBuf,
    output: OutputMode,
) -> anyhow::Result<()> {
    #[cfg(all(not(windows), not(any(target_os = "linux", target_os = "macos"))))]
    {
        let _ = (config_path, target, destination, output);
        anyhow::bail!("operon mount is not supported on this platform");
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    {
        let target = parse_node_path(target)?;
        let endpoint = load_endpoint(config_path, &target.node_id)?;
        let mount = operon_mount::spawn_mount(operon_mount::MountOptions {
            endpoint,
            remote_path: target.path.clone(),
            mount_point: destination.clone(),
        })?;
        let manifest = serde_json::json!({
            "mode": "write-through-live-fuse",
            "node_id": target.node_id,
            "path": target.path,
            "destination": destination,
            "cache": "kernel page cache only",
            "consistency": "live reads and write-through mutations through Operon fs gRPC; metadata cached for one second",
            "write": "single-writer write-through in v0.6.1",
            "adapter": mount_adapter_name(),
        });
        if output.json {
            print_json(&manifest)?;
        } else if !output.quiet {
            println!(
                "mounted {}:{} at {}",
                manifest["node_id"].as_str().unwrap_or_default(),
                manifest["path"].as_str().unwrap_or_default(),
                manifest["destination"].as_str().unwrap_or_default()
            );
            println!("press Ctrl-C to unmount");
        }
        mount.wait_for_shutdown()
    }
}

#[cfg(target_os = "linux")]
fn mount_adapter_name() -> &'static str {
    "linux-fuse"
}

#[cfg(target_os = "macos")]
fn mount_adapter_name() -> &'static str {
    "macos-macfuse"
}

#[cfg(windows)]
fn mount_adapter_name() -> &'static str {
    "windows-winfsp"
}
