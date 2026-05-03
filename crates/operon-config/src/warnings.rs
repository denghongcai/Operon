#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigWarning {
    pub path: String,
}

pub(crate) fn collect_unknown_config_fields(value: &serde_yaml::Value) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();
    collect_object(
        value,
        "",
        &["version", "daemon", "client", "policy", "secrets"],
        &mut warnings,
    );
    collect_object(
        value.get("daemon").unwrap_or(&serde_yaml::Value::Null),
        "daemon",
        &[
            "node_id",
            "grpc_listen",
            "workspace",
            "advertise_lan",
            "store",
            "auth",
        ],
        &mut warnings,
    );
    collect_auth(
        value
            .get("daemon")
            .and_then(|daemon| daemon.get("auth"))
            .unwrap_or(&serde_yaml::Value::Null),
        "daemon.auth",
        &mut warnings,
    );
    collect_object(
        value.get("client").unwrap_or(&serde_yaml::Value::Null),
        "client",
        &["nodes"],
        &mut warnings,
    );
    if let Some(nodes) = value.get("client").and_then(|client| client.get("nodes")) {
        collect_client_nodes(nodes, &mut warnings);
    }
    collect_policy(
        value.get("policy").unwrap_or(&serde_yaml::Value::Null),
        "policy",
        &mut warnings,
    );
    collect_object(
        value.get("secrets").unwrap_or(&serde_yaml::Value::Null),
        "secrets",
        &["file"],
        &mut warnings,
    );
    warnings
}

fn collect_client_nodes(value: &serde_yaml::Value, warnings: &mut Vec<ConfigWarning>) {
    let Some(nodes) = value.as_mapping() else {
        return;
    };
    for (key, node) in nodes {
        let Some(node_id) = key.as_str() else {
            continue;
        };
        let path = format!("client.nodes.{node_id}");
        collect_object(node, &path, &["endpoint", "auth"], warnings);
        collect_auth(
            node.get("auth").unwrap_or(&serde_yaml::Value::Null),
            &format!("{path}.auth"),
            warnings,
        );
    }
}

fn collect_auth(value: &serde_yaml::Value, path: &str, warnings: &mut Vec<ConfigWarning>) {
    collect_object(value, path, &["token", "token_file", "token_env"], warnings);
}

fn collect_policy(value: &serde_yaml::Value, path: &str, warnings: &mut Vec<ConfigWarning>) {
    collect_object(value, path, &["subject", "fs", "exec", "service"], warnings);
    collect_object(
        value.get("fs").unwrap_or(&serde_yaml::Value::Null),
        &format!("{path}.fs"),
        &["mounts"],
        warnings,
    );
    collect_sequence_objects(
        value
            .get("fs")
            .and_then(|fs| fs.get("mounts"))
            .unwrap_or(&serde_yaml::Value::Null),
        &format!("{path}.fs.mounts"),
        &["name", "path", "permissions"],
        warnings,
    );
    if let Some(mounts) = value.get("fs").and_then(|fs| fs.get("mounts")) {
        collect_indexed_child_object(
            mounts,
            &format!("{path}.fs.mounts"),
            "permissions",
            &["read", "write", "delete"],
            warnings,
        );
    }
    collect_object(
        value.get("exec").unwrap_or(&serde_yaml::Value::Null),
        &format!("{path}.exec"),
        &[
            "allowed_cwds",
            "default_timeout_secs",
            "max_timeout_secs",
            "preserve_env",
            "env_allowlist",
            "allowed_secrets",
        ],
        warnings,
    );
    collect_object(
        value.get("service").unwrap_or(&serde_yaml::Value::Null),
        &format!("{path}.service"),
        &["services"],
        warnings,
    );
    collect_sequence_objects(
        value
            .get("service")
            .and_then(|service| service.get("services"))
            .unwrap_or(&serde_yaml::Value::Null),
        &format!("{path}.service.services"),
        &[
            "id",
            "name",
            "host",
            "port",
            "protocol",
            "description",
            "permissions",
        ],
        warnings,
    );
    if let Some(services) = value
        .get("service")
        .and_then(|service| service.get("services"))
    {
        collect_indexed_child_object(
            services,
            &format!("{path}.service.services"),
            "permissions",
            &["check", "forward"],
            warnings,
        );
    }
}

fn collect_sequence_objects(
    value: &serde_yaml::Value,
    path: &str,
    allowed: &[&str],
    warnings: &mut Vec<ConfigWarning>,
) {
    let Some(items) = value.as_sequence() else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        collect_object(item, &format!("{path}[{index}]"), allowed, warnings);
    }
}

fn collect_indexed_child_object(
    value: &serde_yaml::Value,
    path: &str,
    child: &str,
    allowed: &[&str],
    warnings: &mut Vec<ConfigWarning>,
) {
    let Some(items) = value.as_sequence() else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        collect_object(
            item.get(child).unwrap_or(&serde_yaml::Value::Null),
            &format!("{path}[{index}].{child}"),
            allowed,
            warnings,
        );
    }
}

fn collect_object(
    value: &serde_yaml::Value,
    path: &str,
    allowed: &[&str],
    warnings: &mut Vec<ConfigWarning>,
) {
    let Some(mapping) = value.as_mapping() else {
        return;
    };
    for (key, _) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };
        if !allowed.contains(&key) {
            warnings.push(ConfigWarning {
                path: join_config_path(path, key),
            });
        }
    }
}

fn join_config_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
}
