use operon_config::OperonConfig;
use std::path::Path;

const CONFIG: &str = r#"
version: 1
daemon:
  node_id: logical-server
  grpc_listen: 127.0.0.1:7789
  workspace: workspace
  iroh:
    identity_file: server.key
    bind_addrs: [127.0.0.1:0]
    handshake_timeout_secs: 0
    max_connections: 16
client:
  nodes:
    logical-peer:
      endpoint: iroh://0000000000000000000000000000000000000000000000000000000000000000
      transport:
        connect_timeout_secs: 0
        iroh:
          identity_file: client.key
          bind_addrs: [127.0.0.1:0]
          peer_addrs: [127.0.0.1:9000]
          idle_timeout_secs: 0
"#;

#[test]
fn explicit_optional_config_preserves_logical_identity_and_resolves_key_paths() {
    let loaded = OperonConfig::from_str_with_warnings(CONFIG).unwrap();
    assert!(loaded.warnings.is_empty());
    let listener = loaded
        .config
        .daemon
        .as_ref()
        .unwrap()
        .iroh
        .as_ref()
        .unwrap();
    assert_eq!(listener.handshake_timeout_secs, 0);
    assert_eq!(listener.max_connections, 16);
    let endpoint = loaded
        .config
        .endpoint("logical-peer", Path::new("cfg"))
        .unwrap();
    assert_eq!(endpoint.node_id, "logical-peer");
    assert_eq!(endpoint.transport.connect_timeout_secs, 0);
    let iroh = endpoint.transport.iroh.unwrap();
    assert_eq!(iroh.local.identity_file, Path::new("cfg/client.key"));
    assert_eq!(iroh.local.idle_timeout_secs, 0);
    assert!(iroh.peer_relay_urls.is_empty());
}

#[test]
fn unknown_embedded_fields_warn_at_the_actual_nested_path() {
    let config = CONFIG
        .replace(
            "max_connections: 16",
            "max_connections: 16\n    unknown_limit: 5",
        )
        .replace(
            "idle_timeout_secs: 0",
            "idle_timeout_secs: 0\n          unknown_deadline: 5",
        );
    let loaded = OperonConfig::from_str_with_warnings(&config).unwrap();
    assert_eq!(
        loaded
            .warnings
            .iter()
            .map(|w| w.path.as_str())
            .collect::<Vec<_>>(),
        [
            "daemon.iroh.unknown_limit",
            "client.nodes.logical-peer.transport.iroh.unknown_deadline"
        ]
    );
}

#[test]
fn invalid_or_dangling_backend_configuration_never_falls_back() {
    for config in [
        CONFIG.replace("iroh://", "grpc://"),
        CONFIG.replace("identity_file: client.key", "identity_file: ''"),
        CONFIG.replace("peer_addrs: [127.0.0.1:9000]", "peer_addrs: []"),
        CONFIG.replace("max_connections: 16", "max_connections: 0"),
        CONFIG.replace(
            "handshake_timeout_secs: 0",
            "handshake_timeout_secs: 604801",
        ),
    ] {
        assert!(OperonConfig::from_str_with_warnings(&config).is_err());
    }
    assert!(OperonConfig::from_str_with_warnings(
        "version: 1\nclient:\n  nodes:\n    peer:\n      endpoint: iroh://invalid\n"
    )
    .is_err());
}

#[test]
fn old_config_remains_disabled_and_serialization_omits_new_options() {
    let loaded = OperonConfig::from_str_with_warnings(
        "version: 1\nclient:\n  nodes:\n    peer:\n      endpoint: grpc://127.0.0.1:7789\n",
    )
    .unwrap();
    let endpoint = loaded.config.endpoint("peer", Path::new(".")).unwrap();
    assert!(endpoint.transport.iroh.is_none());
    assert!(!serde_yaml::to_string(&loaded.config)
        .unwrap()
        .contains("iroh:"));
}
