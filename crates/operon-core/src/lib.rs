pub mod audit;
pub mod discovery;
pub mod exec;
pub mod fs;
pub mod policy;
pub mod runtime;
pub mod service;
pub mod trace;

pub use audit::*;
pub use discovery::*;
pub use exec::*;
pub use fs::*;
pub use policy::*;
pub use runtime::*;
pub use service::*;
pub use trace::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_status_uses_kebab_case_wire_names() {
        assert_eq!(
            serde_json::to_string(&ExecStatus::TimedOut).expect("serialize"),
            "\"timed-out\""
        );
    }

    #[test]
    fn policy_config_round_trips_from_yaml() {
        let policy: PolicyConfig = serde_yaml::from_str(
            r#"
subject: local-cli
fs:
  mounts:
    - name: workspace
      path: /
      permissions:
        read: true
        write: true
        delete: false
exec:
  allowed_cwds:
    - /
  default_timeout_secs: 30
  max_timeout_secs: 300
  preserve_env: false
  env_allowlist:
    - GITHUB_TOKEN
"#,
        )
        .expect("policy should parse");

        assert_eq!(policy.subject, "local-cli");
        assert_eq!(policy.fs.mounts[0].name, "workspace");
        assert!(policy.fs.mounts[0].permissions.read);
        assert!(!policy.fs.mounts[0].permissions.delete);
        assert_eq!(policy.exec.max_timeout_secs, 300);
        assert!(!policy.exec.preserve_env);
        assert_eq!(policy.exec.env_allowlist, vec!["GITHUB_TOKEN"]);
        assert!(policy.service.services.is_empty());
    }

    #[test]
    fn service_policy_parses_allowed_services() {
        let policy: PolicyConfig = serde_yaml::from_str(
            r#"
subject: local-cli
fs:
  mounts: []
exec:
  allowed_cwds:
    - /
  default_timeout_secs: 30
  max_timeout_secs: 300
  env_allowlist: []
service:
  services:
    - id: app
      name: app
      host: 127.0.0.1
      port: 8080
      protocol: tcp
      description: local app
      permissions:
        check: true
        forward: true
    - id: dns
      name: dns
      host: 127.0.0.1
      port: 5353
      protocol: udp
      description: local dns
      permissions:
        check: true
        forward: false
"#,
        )
        .expect("policy should parse");

        assert_eq!(policy.service.services[0].id, "app");
        assert!(matches!(
            policy.service.services[0].protocol,
            ServiceProtocol::Tcp
        ));
        assert_eq!(policy.service.services[1].id, "dns");
        assert!(matches!(
            policy.service.services[1].protocol,
            ServiceProtocol::Udp
        ));
        assert!(policy.service.services[0].permissions.forward);
        assert!(!policy.service.services[1].permissions.forward);
    }

    #[test]
    fn execution_graph_yaml_supports_mvp_step_fields() {
        let graph: ExecutionGraph = serde_yaml::from_str(
            r#"
name: copy-and-run
steps:
  - id: write-input
    node: node-a
    action: fs.write
    path: /input.txt
    content: hello
  - id: run-command
    node: node-a
    action: exec.run
    cwd: /
    timeout_secs: 5
    command: cat input.txt
"#,
        )
        .expect("graph should parse");

        assert_eq!(graph.name, "copy-and-run");
        assert_eq!(graph.steps.len(), 2);
        assert_eq!(graph.steps[0].content.as_deref(), Some("hello"));
        assert_eq!(graph.steps[1].timeout_secs, Some(5));
    }

    #[test]
    fn domain_module_paths_and_root_reexports_match() {
        let root_status = ExecStatus::TimedOut;
        let module_status = exec::ExecStatus::TimedOut;

        assert_eq!(
            serde_json::to_string(&root_status).expect("serialize root"),
            serde_json::to_string(&module_status).expect("serialize module")
        );

        let policy = policy::PolicyConfig {
            subject: "local-cli".to_string(),
            fs: FsPolicy { mounts: vec![] },
            exec: ExecPolicy {
                allowed_cwds: vec!["/tmp".to_string()],
                default_timeout_secs: 30,
                max_timeout_secs: 300,
                preserve_env: false,
                env_allowlist: vec![],
                allowed_secrets: vec![],
            },
            service: service::ServicePolicy::default(),
        };

        assert_eq!(policy.subject, "local-cli");
    }
}
