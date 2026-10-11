//! Native live-test helper; not a distributed user-facing command.
#[cfg(any(target_os = "linux", windows))]
fn main() -> anyhow::Result<()> {
    use std::io::Write;
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 5 || args.len() == 8,
        "expected endpoint, token file, mount point, read mode and optional pending/concurrency/budget"
    );
    let mut reads = operon_core::runtime::MountReadConfig {
        mode: args[4].parse().map_err(anyhow::Error::msg)?,
        ..Default::default()
    };
    if args.len() == 8 {
        reads.max_pending_reads = args[5].parse()?;
        reads.max_inflight_reads = args[6].parse()?;
        reads.max_inflight_read_mib = args[7].parse()?;
    }
    let session = operon_mount::spawn_mount(operon_mount::MountOptions {
        endpoint: operon_core::runtime::NodeEndpoint {
            node_id: "local".into(),
            endpoint: args[1].clone(),
            token: Some(std::fs::read_to_string(&args[2])?.trim().to_owned()),
            transport: operon_core::runtime::TransportConfig {
                rpc_timeout_secs: 0,
                ..Default::default()
            },
        },
        remote_path: "/".into(),
        mount_point: (&args[3]).into(),
        reads,
    })?;
    println!("ready");
    std::io::stdout().flush()?;
    let mut action = String::new();
    let action_bytes = std::io::stdin().read_line(&mut action)?;
    match action.trim() {
        "drop" => drop(session),
        "unmount" => session.unmount()?,
        _ => anyhow::bail!("expected drop or unmount; received {action_bytes} bytes: {action:?}"),
    }
    #[cfg(target_os = "linux")]
    for thread in std::fs::read_dir("/proc/self/task")? {
        let name = std::fs::read_to_string(thread?.path().join("comm"))?;
        anyhow::ensure!(
            !name.trim().starts_with("fuser-"),
            "FUSE thread survived session cleanup"
        );
    }
    #[cfg(target_os = "linux")]
    for descriptor in std::fs::read_dir("/proc/self/fd")? {
        if let Ok(target) = std::fs::read_link(descriptor?.path()) {
            anyhow::ensure!(
                target != std::path::Path::new("/dev/fuse"),
                "FUSE descriptor survived session cleanup"
            );
        }
    }
    println!("joined");
    Ok(())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn main() {
    eprintln!("session-lifecycle supports Linux and Windows live tests");
    std::process::exit(1);
}
