use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};

use clap::ValueEnum;
use operon_config::{
    AuthConfig, ClientConfig, DaemonConfig, NetworkProviderKind, NodeConfig, OperonConfig,
    SecretsConfig,
};
use operon_core::DiscoveryList;

use crate::{private_files, OutputMode};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum OnboardRole {
    Daemon,
    Client,
    Both,
}

#[derive(Debug, clap::Args)]
pub(crate) struct OnboardArgs {
    #[arg(long, value_enum, default_value_t = OnboardRole::Both)]
    role: OnboardRole,

    #[arg(long, default_value = ".operon")]
    output_dir: PathBuf,

    #[arg(long, default_value = "local")]
    node_id: String,

    #[arg(long, default_value = "/workspace")]
    workspace: String,

    #[arg(long, default_value = "0.0.0.0:7789")]
    listen: String,

    #[arg(long)]
    endpoint: Option<String>,

    #[arg(long)]
    token: Option<String>,

    #[arg(long)]
    discover_lan: bool,

    #[arg(long, default_value_t = 3)]
    timeout_secs: u64,

    #[arg(long)]
    allow_all: bool,

    #[arg(long)]
    yes: bool,
}

#[derive(Debug, serde::Serialize)]
struct OnboardSummary {
    role: String,
    output_dir: String,
    files: Vec<String>,
    daemon_command: Option<String>,
    equivalent_cli: Vec<String>,
    completion_commands: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct CapabilityGrant {
    fs_read: bool,
    fs_write: bool,
    fs_delete: bool,
    job_run: bool,
    service_check: bool,
}

impl CapabilityGrant {
    fn default_guided() -> Self {
        Self {
            fs_read: true,
            fs_write: true,
            fs_delete: false,
            job_run: true,
            service_check: true,
        }
    }

    fn all() -> Self {
        Self {
            fs_read: true,
            fs_write: true,
            fs_delete: true,
            job_run: true,
            service_check: true,
        }
    }
}

pub(crate) fn run(args: OnboardArgs, output: OutputMode) -> anyhow::Result<()> {
    let mut prompt = StdioPrompt;
    let plan = build_onboard_plan(args, &mut prompt)?;

    fs::create_dir_all(&plan.output_dir)?;
    for file in &plan.files {
        write_generated_file(file)?;
    }

    if output.json {
        crate::print_json(&plan.summary())?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

    println!("Wrote:");
    for file in &plan.files {
        println!("  {}", file.path.display());
    }

    if let Some(command) = &plan.daemon_command {
        println!();
        println!("Next:");
        println!("  {command}");
    }

    println!();
    println!("Equivalent CLI:");
    for command in &plan.equivalent_cli {
        println!("  {command}");
    }

    println!();
    println!("Shell completion:");
    for command in &plan.completion_commands {
        println!("  {command}");
    }

    Ok(())
}

struct OnboardPlan {
    output_dir: PathBuf,
    files: Vec<GeneratedFile>,
    daemon_command: Option<String>,
    equivalent_cli: Vec<String>,
    completion_commands: Vec<String>,
    role: OnboardRole,
}

impl OnboardPlan {
    fn summary(&self) -> OnboardSummary {
        OnboardSummary {
            role: format!("{:?}", self.role).to_ascii_lowercase(),
            output_dir: self.output_dir.display().to_string(),
            files: self
                .files
                .iter()
                .map(|file| file.path.display().to_string())
                .collect(),
            daemon_command: self.daemon_command.clone(),
            equivalent_cli: self.equivalent_cli.clone(),
            completion_commands: self.completion_commands.clone(),
        }
    }
}

struct GeneratedFile {
    path: PathBuf,
    content: String,
    private: bool,
}

trait Prompt {
    fn input(&mut self, label: &str, default: &str) -> anyhow::Result<String>;
    fn confirm(&mut self, label: &str, default: bool) -> anyhow::Result<bool>;
}

struct StdioPrompt;

impl Prompt for StdioPrompt {
    fn input(&mut self, label: &str, default: &str) -> anyhow::Result<String> {
        print!("{label} [{default}]: ");
        io::stdout().flush()?;
        let mut value = String::new();
        io::stdin().read_line(&mut value)?;
        let value = value.trim();
        if value.is_empty() {
            Ok(default.to_string())
        } else {
            Ok(value.to_string())
        }
    }

    fn confirm(&mut self, label: &str, default: bool) -> anyhow::Result<bool> {
        let marker = if default { "Y/n" } else { "y/N" };
        print!("{label} [{marker}]: ");
        io::stdout().flush()?;
        let mut value = String::new();
        io::stdin().read_line(&mut value)?;
        let value = value.trim().to_ascii_lowercase();
        if value.is_empty() {
            return Ok(default);
        }
        Ok(matches!(value.as_str(), "y" | "yes"))
    }
}

fn build_onboard_plan(args: OnboardArgs, prompt: &mut impl Prompt) -> anyhow::Result<OnboardPlan> {
    let role = args.role;
    let interactive = !args.yes;
    let output_dir = if interactive {
        PathBuf::from(prompt.input("Output directory", &args.output_dir.display().to_string())?)
    } else {
        args.output_dir
    };
    let node_id = if interactive && matches!(role, OnboardRole::Daemon | OnboardRole::Both) {
        prompt.input("Local node id", &args.node_id)?
    } else {
        args.node_id
    };
    let workspace = if interactive && matches!(role, OnboardRole::Daemon | OnboardRole::Both) {
        prompt.input("Daemon workspace path", &args.workspace)?
    } else {
        args.workspace
    };
    let listen = if interactive && matches!(role, OnboardRole::Daemon | OnboardRole::Both) {
        prompt.input("Daemon listen address", &args.listen)?
    } else {
        args.listen
    };
    let endpoint = args
        .endpoint
        .unwrap_or_else(|| endpoint_from_listen_address(&listen));
    let provided_token = args.token;
    let daemon_token = if matches!(role, OnboardRole::Daemon | OnboardRole::Both) {
        Some(match provided_token.clone() {
            Some(token) => token,
            None => private_files::generate_token()?,
        })
    } else {
        None
    };
    let client_token = if matches!(role, OnboardRole::Client | OnboardRole::Both) {
        match (provided_token, daemon_token.clone()) {
            (Some(token), _) => Some(token),
            (None, Some(token)) => Some(token),
            (None, None) if interactive => {
                let token = prompt.input("Node auth token (empty for none)", "")?;
                if token.is_empty() {
                    None
                } else {
                    Some(token)
                }
            }
            (None, None) => None,
        }
    } else {
        None
    };
    let grant = if args.allow_all {
        CapabilityGrant::all()
    } else if interactive && matches!(role, OnboardRole::Daemon | OnboardRole::Both) {
        prompt_capability_grants(prompt)?
    } else {
        CapabilityGrant::default_guided()
    };
    let discover_lan = if interactive && matches!(role, OnboardRole::Client | OnboardRole::Both) {
        prompt.confirm("Discover LAN nodes now", args.discover_lan)?
    } else {
        args.discover_lan
    };

    let mut files = Vec::new();
    let mut equivalent_cli = Vec::new();
    let mut daemon_command = None;
    let token_path = output_dir.join("token");
    let config_path = output_dir.join("config.yaml");
    let daemon_readme_path = output_dir.join("daemon-command.txt");
    let mut daemon = None;
    let mut policy = None;
    let mut nodes = BTreeMap::new();

    if matches!(role, OnboardRole::Daemon | OnboardRole::Both) {
        let token = daemon_token
            .as_ref()
            .expect("daemon onboarding should have a token");
        files.push(GeneratedFile {
            path: token_path.clone(),
            content: format!("{token}\n"),
            private: true,
        });
        policy = Some(build_policy(
            &node_id,
            &grant,
            service_port_from_listen_address(&listen),
        )?);
        daemon = Some(DaemonConfig {
            node_id: node_id.clone(),
            grpc_listen: listen.parse()?,
            workspace: PathBuf::from(&workspace),
            advertise_lan: true,
            store: Some(PathBuf::from("store.jsonl")),
            auth: AuthConfig {
                token: None,
                token_file: Some(PathBuf::from("token")),
                token_env: None,
            },
        });
        let command = format!("operond start --config {}", config_path.display());
        files.push(GeneratedFile {
            path: daemon_readme_path,
            content: format!("{command}\n"),
            private: false,
        });
        daemon_command = Some(command);
    }

    if matches!(role, OnboardRole::Client | OnboardRole::Both) {
        nodes.insert(
            node_id.clone(),
            NodeConfig {
                endpoint: endpoint.clone(),
                provider: NetworkProviderKind::Manual,
                auth: client_token_auth(client_token.clone(), matches!(role, OnboardRole::Both)),
            },
        );

        if discover_lan {
            let discovered = discover_lan_nodes(Duration::from_secs(args.timeout_secs))?;
            for node in discovered.nodes {
                nodes.entry(node.node_id).or_insert(NodeConfig {
                    endpoint: node.endpoint,
                    provider: NetworkProviderKind::Lan,
                    auth: client_token_auth(
                        client_token.clone(),
                        matches!(role, OnboardRole::Both),
                    ),
                });
            }
            equivalent_cli.push(format!(
                "operon node discover --provider lan --output-config {}",
                config_path.display()
            ));
        }
        equivalent_cli.push(format!("operon init config {}", config_path.display()));
    }

    files.push(GeneratedFile {
        path: config_path.clone(),
        content: serde_yaml::to_string(&OperonConfig {
            version: 1,
            daemon,
            client: ClientConfig { nodes },
            policy,
            secrets: Some(SecretsConfig::default()),
        })?,
        private: false,
    });

    Ok(OnboardPlan {
        output_dir,
        files,
        daemon_command,
        equivalent_cli,
        completion_commands: completion_setup_commands(),
        role,
    })
}

fn completion_setup_commands() -> Vec<String> {
    vec![
        "mkdir -p ~/.local/share/bash-completion/completions && operon completion bash > ~/.local/share/bash-completion/completions/operon".to_string(),
        "mkdir -p ~/.zfunc && operon completion zsh > ~/.zfunc/_operon".to_string(),
    ]
}

fn prompt_capability_grants(prompt: &mut impl Prompt) -> anyhow::Result<CapabilityGrant> {
    let all = prompt.confirm("Allow all MVP capabilities", false)?;
    if all {
        return Ok(CapabilityGrant::all());
    }

    Ok(CapabilityGrant {
        fs_read: prompt.confirm("Allow filesystem read", true)?,
        fs_write: prompt.confirm("Allow filesystem write", true)?,
        fs_delete: prompt.confirm("Allow filesystem delete/rename", false)?,
        job_run: prompt.confirm("Allow job run", true)?,
        service_check: prompt.confirm("Allow service checks", true)?,
    })
}

#[cfg(test)]
fn render_policy(subject: &str, grant: &CapabilityGrant, service_port: u16) -> String {
    serde_yaml::to_string(&build_policy(subject, grant, service_port).expect("policy")).unwrap()
}

fn build_policy(
    subject: &str,
    grant: &CapabilityGrant,
    service_port: u16,
) -> anyhow::Result<operon_core::PolicyConfig> {
    serde_yaml::from_str(&render_policy_yaml(subject, grant, service_port)).map_err(Into::into)
}

fn render_policy_yaml(subject: &str, grant: &CapabilityGrant, service_port: u16) -> String {
    let default_timeout_secs = if grant.job_run { 30 } else { 1 };
    let max_timeout_secs = if grant.job_run { 300 } else { 1 };
    let services = render_services(grant, service_port);
    format!(
        r#"subject: {subject}

fs:
  mounts:
    - name: workspace
      path: /
      permissions:
        read: {fs_read}
        write: {fs_write}
        delete: {fs_delete}

job:
  allowed_cwds:
    - /
  default_timeout_secs: {default_timeout_secs}
  max_timeout_secs: {max_timeout_secs}
  preserve_env: false
  env_allowlist: []
  allowed_secrets: []

service:
  services:{services}"#,
        fs_read = grant.fs_read,
        fs_write = grant.fs_write,
        fs_delete = grant.fs_delete,
    )
}

fn client_token_auth(token: Option<String>, local_token_file: bool) -> AuthConfig {
    if local_token_file {
        AuthConfig {
            token: None,
            token_file: Some(PathBuf::from("token")),
            token_env: None,
        }
    } else {
        AuthConfig {
            token,
            token_file: None,
            token_env: None,
        }
    }
}

fn render_services(grant: &CapabilityGrant, service_port: u16) -> String {
    if grant.service_check {
        format!(
            r#"
    - id: local-daemon
      name: local-daemon
      host: 127.0.0.1
      port: {service_port}
      protocol: tcp
      description: Operon gRPC daemon listener
      permissions:
        check: true
        forward: true
"#
        )
    } else {
        " []\n".to_string()
    }
}

fn endpoint_from_listen_address(listen: &str) -> String {
    let port = listen_port(listen).unwrap_or("7789");
    let host = listen
        .rsplit_once(':')
        .map(|(host, _)| host)
        .filter(|host| !host.is_empty() && *host != "0.0.0.0" && *host != "::")
        .unwrap_or("127.0.0.1");
    format!("grpc://{host}:{port}")
}

fn service_port_from_listen_address(listen: &str) -> u16 {
    listen_port(listen)
        .and_then(|port| port.parse().ok())
        .unwrap_or(7789)
}

fn listen_port(listen: &str) -> Option<&str> {
    listen
        .rsplit_once(':')
        .map(|(_, port)| port)
        .filter(|port| !port.is_empty())
}

fn write_generated_file(file: &GeneratedFile) -> anyhow::Result<()> {
    if file.private {
        return private_files::write_private_file(&file.path, &file.content);
    }
    fs::write(&file.path, &file.content)?;
    Ok(())
}

fn discover_lan_nodes(timeout: Duration) -> anyhow::Result<DiscoveryList> {
    operon_network::discover_lan_nodes(timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    struct NoopPrompt;

    impl Prompt for NoopPrompt {
        fn input(&mut self, _label: &str, default: &str) -> anyhow::Result<String> {
            Ok(default.to_string())
        }

        fn confirm(&mut self, _label: &str, default: bool) -> anyhow::Result<bool> {
            Ok(default)
        }
    }

    fn test_args(role: OnboardRole) -> OnboardArgs {
        OnboardArgs {
            role,
            output_dir: PathBuf::from("/tmp/operon-test"),
            node_id: "node-a".to_string(),
            workspace: "/workspace".to_string(),
            listen: "127.0.0.1:17789".to_string(),
            endpoint: None,
            token: None,
            discover_lan: false,
            timeout_secs: 1,
            allow_all: false,
            yes: true,
        }
    }

    #[test]
    fn derives_local_endpoint_from_wildcard_listen_address() {
        assert_eq!(
            endpoint_from_listen_address("0.0.0.0:7789"),
            "grpc://127.0.0.1:7789"
        );
    }

    #[test]
    fn renders_policy_with_selected_grants() {
        let policy = render_policy(
            "node-a",
            &CapabilityGrant {
                fs_read: true,
                fs_write: false,
                fs_delete: true,
                job_run: true,
                service_check: false,
            },
            17789,
        );

        assert!(policy.contains("subject: node-a"));
        assert!(policy.contains("read: true"));
        assert!(policy.contains("write: false"));
        assert!(policy.contains("delete: true"));
        assert!(!policy.contains("local-daemon"));
        let parsed: operon_core::PolicyConfig =
            serde_yaml::from_str(&policy).expect("policy should parse");
        assert!(parsed.service.services.is_empty());
    }

    #[test]
    fn renders_policy_service_port_from_listen_address() {
        let policy = render_policy("node-a", &CapabilityGrant::default_guided(), 17789);

        assert!(policy.contains("port: 17789"));
    }

    #[test]
    fn generated_token_is_hex_encoded() {
        let token = private_files::generate_token().expect("token");
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|value| value.is_ascii_hexdigit()));
    }

    #[test]
    fn client_only_noninteractive_does_not_invent_token() {
        let plan =
            build_onboard_plan(test_args(OnboardRole::Client), &mut NoopPrompt).expect("plan");
        let nodes = plan
            .files
            .iter()
            .find(|file| file.path.ends_with("config.yaml"))
            .expect("config file");

        assert!(!nodes.content.contains("token:"));
    }

    #[test]
    fn onboard_summary_includes_shell_completion_commands() {
        let plan = build_onboard_plan(test_args(OnboardRole::Both), &mut NoopPrompt).expect("plan");
        let summary = plan.summary();

        assert!(summary
            .completion_commands
            .iter()
            .any(|command| command.contains("operon completion bash")));
        assert!(summary
            .completion_commands
            .iter()
            .any(|command| command.contains("operon completion zsh")));
    }

    #[cfg(unix)]
    #[test]
    fn private_generated_file_refuses_broad_existing_permissions() {
        let base = std::env::temp_dir().join(format!(
            "operon-onboard-private-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&base).expect("dir");
        let path = base.join("token");
        fs::write(&path, "old\n").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

        let error = write_generated_file(&GeneratedFile {
            path: path.clone(),
            content: "new\n".to_string(),
            private: true,
        })
        .expect_err("broad token file should be rejected");

        assert!(error.to_string().contains("refusing to write private file"));
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn private_generated_file_is_written_with_owner_only_permissions() {
        let base = std::env::temp_dir().join(format!(
            "operon-onboard-private-write-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&base).expect("dir");
        let path = base.join("token");

        write_generated_file(&GeneratedFile {
            path: path.clone(),
            content: "new\n".to_string(),
            private: true,
        })
        .expect("write private file");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(base);
    }
}
