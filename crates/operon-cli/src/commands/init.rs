use std::{fs, path::PathBuf};

use operon_config::OperonConfig;

use crate::{
    output::{print_json, OutputMode},
    private_files,
};

#[derive(Debug, serde::Serialize)]
struct InitConfigSummary {
    config: String,
    token: String,
    secrets: String,
}

pub(crate) fn init_config(path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let config_dir = OperonConfig::config_dir(&path);
    fs::create_dir_all(&config_dir)?;
    let token_path = config_dir.join("token");
    let secrets_path = config_dir.join("secrets.yaml");
    let content = r#"version: 1

daemon:
  node_id: local
  grpc_listen: 127.0.0.1:7789
  workspace: /workspace
  advertise_lan: false
  store: store.jsonl
  auth:
    token_file: token

client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:7789
      provider: manual
      auth:
        token_file: token

policy:
  subject: local-cli
  fs:
    mounts:
      - name: workspace
        path: /
        permissions:
          read: true
          write: true
          delete: false
  job:
    allowed_cwds:
      - /
    default_timeout_secs: 30
    max_timeout_secs: 300
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
  service:
    services:
      - id: local-daemon
        name: local-daemon
        host: 127.0.0.1
        port: 7789
        protocol: tcp
        description: Operon gRPC daemon listener
        permissions:
          check: true
          forward: true

secrets:
  file: secrets.yaml
"#;
    fs::write(&path, content)?;
    private_files::write_private_file(
        &token_path,
        &format!("{}\n", private_files::generate_token()?),
    )?;
    private_files::write_private_file(&secrets_path, "{}\n")?;
    if output.json {
        print_json(&InitConfigSummary {
            config: path.display().to_string(),
            token: token_path.display().to_string(),
            secrets: secrets_path.display().to_string(),
        })?;
        return Ok(());
    }
    if !output.quiet {
        println!("{}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs};

    use super::*;

    #[test]
    fn init_config_writes_referenced_starter_files() {
        let base = unique_temp_dir("operon-init-config-test");
        let config_path = base.join("config.yaml");

        init_config(
            config_path.clone(),
            OutputMode {
                json: false,
                quiet: true,
            },
        )
        .expect("init config");

        let config = OperonConfig::load(&config_path).expect("config should load");
        let config_dir = OperonConfig::config_dir(&config_path);
        let token = config
            .daemon
            .as_ref()
            .expect("daemon")
            .auth
            .resolve(&config_dir)
            .expect("daemon token")
            .expect("daemon token value");
        let endpoint = config
            .endpoint("local", &config_dir)
            .expect("client endpoint");
        let secrets: BTreeMap<String, String> =
            serde_yaml::from_str(&fs::read_to_string(base.join("secrets.yaml")).expect("secrets"))
                .expect("secrets yaml");

        assert_eq!(token.len(), 64);
        assert_eq!(endpoint.token.as_deref(), Some(token.as_str()));
        assert!(secrets.is_empty());
        let _ = fs::remove_dir_all(base);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ))
    }
}
