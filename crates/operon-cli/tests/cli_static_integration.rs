use std::{fs, path::PathBuf, process::Command};

fn operon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_operon"))
}

#[test]
fn help_lists_self_description_and_completion_command() {
    let output = operon().arg("--help").output().expect("run operon help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 help");
    assert!(stdout.contains("config.yaml"));
    assert!(stdout.contains("completion"));
}

#[test]
fn completion_generation_supports_bash_and_zsh() {
    let bash = operon()
        .args(["completion", "bash"])
        .output()
        .expect("run bash completion");
    assert!(bash.status.success());
    let bash_stdout = String::from_utf8(bash.stdout).expect("utf8 bash completion");
    assert!(bash_stdout.contains("complete -F"));

    let zsh = operon()
        .args(["completion", "zsh"])
        .output()
        .expect("run zsh completion");
    assert!(zsh.status.success());
    let zsh_stdout = String::from_utf8(zsh.stdout).expect("utf8 zsh completion");
    assert!(zsh_stdout.contains("#compdef operon"));
}

#[test]
fn init_config_then_explain_json_is_machine_readable() {
    let base = unique_temp_dir("operon-cli-static-integration");
    let config = base.join("config.yaml");

    let init = operon()
        .args(["--quiet", "init", "config"])
        .arg(&config)
        .output()
        .expect("run init config");
    assert!(init.status.success(), "stderr={}", stderr(&init));

    let explain = operon()
        .arg("--config")
        .arg(&config)
        .args(["--json", "config", "explain"])
        .output()
        .expect("run config explain");
    assert!(explain.status.success(), "stderr={}", stderr(&explain));

    let json: serde_json::Value =
        serde_json::from_slice(&explain.stdout).expect("config explain json");
    assert_eq!(json["daemon"]["node_id"], "local");
    assert_eq!(json["client"]["nodes"][0]["node_id"], "local");
    assert_eq!(json["policy"]["fs_mounts"][0]["name"], "workspace");
    assert!(json["daemon"]["auth"]
        .as_str()
        .expect("auth string")
        .starts_with("token_file:"));

    let _ = fs::remove_dir_all(base);
}

#[test]
fn config_unknown_fields_warn_without_blocking_command() {
    let base = unique_temp_dir("operon-cli-config-warning");
    let config = base.join("config.yaml");
    fs::write(
        &config,
        r#"
version: 1
client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:7789
      provider: tailscale
"#,
    )
    .expect("write config");

    let output = operon()
        .arg("--config")
        .arg(&config)
        .args(["node", "list"])
        .output()
        .expect("run node list");
    assert!(output.status.success(), "stderr={}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("local\tgrpc://127.0.0.1:7789"));
    assert!(stderr(&output).contains("client.nodes.local.provider"));

    let _ = fs::remove_dir_all(base);
}

#[test]
fn onboard_summary_includes_completion_guidance() {
    let base = unique_temp_dir("operon-cli-onboard-integration");

    let output = operon()
        .args(["onboard", "--yes", "--output-dir"])
        .arg(&base)
        .output()
        .expect("run onboard");
    assert!(output.status.success(), "stderr={}", stderr(&output));
    let stdout = String::from_utf8(output.stdout).expect("utf8 onboard output");
    assert!(stdout.contains("Shell completion:"));
    assert!(stdout.contains("operon completion bash"));
    assert!(stdout.contains("operon completion zsh"));

    let _ = fs::remove_dir_all(base);
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
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
