use std::path::{Path, PathBuf};

use operon_config::OperonConfig;

use crate::output::{print_json, OutputMode};

use super::ConfigExplain;

pub(crate) fn explain(config_path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let config = OperonConfig::load(&config_path)?;
    let explain = build_config_explain(&config_path, &config);
    if output.json {
        print_json(&explain)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    print_config_explain(&explain);
    Ok(())
}

fn build_config_explain(config_path: &Path, config: &OperonConfig) -> ConfigExplain {
    ConfigExplain::from_config(config_path, config)
}

fn print_config_explain(explain: &ConfigExplain) {
    println!("config: {}", explain.path);
    println!("default_path: {}", explain.default_path);
    println!("config_dir: {}", explain.config_dir);

    match &explain.daemon {
        Some(daemon) => {
            println!("daemon:");
            println!("  node_id: {}", daemon.node_id);
            println!("  grpc_listen: {}", daemon.grpc_listen);
            println!("  workspace: {}", daemon.workspace);
            println!("  advertise_lan: {}", daemon.advertise_lan);
            println!("  store: {}", daemon.store.as_deref().unwrap_or("<none>"));
            println!("  auth: {}", daemon.auth);
        }
        None => println!("daemon: <none>"),
    }

    println!("client nodes:");
    if explain.client.nodes.is_empty() {
        println!("  <none>");
    }
    for node in &explain.client.nodes {
        println!(
            "  {} -> {} (auth: {})",
            node.node_id, node.endpoint, node.auth
        );
    }

    match &explain.policy {
        Some(policy) => {
            println!("policy:");
            println!("  subject: {}", policy.subject);
            println!("  fs mounts:");
            if policy.fs_mounts.is_empty() {
                println!("    <none>");
            }
            for mount in &policy.fs_mounts {
                println!(
                    "    {} path={} read={} write={} delete={}",
                    mount.name, mount.path, mount.read, mount.write, mount.delete
                );
            }
            println!(
                "  exec: allowed_cwds={} default_timeout={} max_timeout={} allow_sessions={} preserve_env={} env_allowlist={} allowed_secrets={}",
                policy.exec.allowed_cwds.join(","),
                policy.exec.default_timeout_secs,
                policy.exec.max_timeout_secs,
                policy.exec.allow_sessions,
                policy.exec.preserve_env,
                policy.exec.env_allowlist.join(","),
                policy.exec.allowed_secrets.join(",")
            );
            println!("  services:");
            if policy.services.is_empty() {
                println!("    <none>");
            }
            for service in &policy.services {
                println!(
                    "    {} {} {} {} check={} forward={} - {}",
                    service.id,
                    service.protocol,
                    service.endpoint,
                    service.name,
                    service.check,
                    service.forward,
                    service.description
                );
            }
            println!("  effective grants:");
            if policy.effective_grants.is_empty() {
                println!("    <none>");
            }
            for grant in &policy.effective_grants {
                println!(
                    "    {} {} {} allowed={} reason={}",
                    grant.capability_id,
                    grant.action,
                    grant.resource,
                    grant.allowed,
                    grant.reason_code
                );
            }
        }
        None => println!("policy: <none>"),
    }

    match &explain.secrets {
        Some(secrets) => println!("secrets: {}", secrets.file.as_deref().unwrap_or("<none>")),
        None => println!("secrets: <none>"),
    }
}
