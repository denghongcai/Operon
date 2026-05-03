use std::collections::BTreeMap;

use operon_core::{ExecPolicy, PolicyDecision, PolicyReasonCode, RuntimeResult};

pub const PROCESS_CAPABILITY: &str = "process";
pub const EXEC_CAPABILITY: &str = "exec";

pub fn authorize_exec(
    policy: &ExecPolicy,
    cwd: &str,
    timeout_secs: Option<u64>,
) -> RuntimeResult<()> {
    let decision = authorize_exec_decision("", policy, cwd, timeout_secs);
    if decision.allowed {
        return Ok(());
    }
    Err(decision.runtime_error())
}

pub fn authorize_exec_session_decision(
    subject: &str,
    policy: &ExecPolicy,
    cwd: &str,
    timeout_secs: Option<u64>,
) -> PolicyDecision {
    if !policy.allow_sessions {
        return PolicyDecision::denied(
            subject,
            "exec:default",
            "session",
            cwd,
            PolicyReasonCode::ExecSessionDenied,
            "exec sessions are denied by policy",
        );
    }
    let decision = authorize_exec_decision(subject, policy, cwd, timeout_secs);
    if decision.allowed {
        return PolicyDecision::allowed(subject, "exec:default", "session", cwd, "allowed");
    }
    PolicyDecision {
        action: "session".to_string(),
        ..decision
    }
}

pub fn authorize_exec_decision(
    subject: &str,
    policy: &ExecPolicy,
    cwd: &str,
    timeout_secs: Option<u64>,
) -> PolicyDecision {
    if !policy
        .allowed_cwds
        .iter()
        .any(|allowed| path_in_policy_scope(cwd, allowed))
    {
        return PolicyDecision::denied(
            subject,
            "exec:default",
            "run",
            cwd,
            PolicyReasonCode::ExecCwdDenied,
            "exec cwd denied by policy",
        );
    }
    if let Some(timeout_secs) = timeout_secs {
        if timeout_secs > policy.max_timeout_secs {
            return PolicyDecision::denied(
                subject,
                "exec:default",
                "run",
                cwd,
                PolicyReasonCode::ExecTimeoutExceeded,
                format!(
                    "exec timeout {timeout_secs}s exceeds policy maximum {}s",
                    policy.max_timeout_secs
                ),
            );
        }
    }
    PolicyDecision::allowed(subject, "exec:default", "run", cwd, "allowed")
}

pub fn resolve_exec_secrets(
    policy: &ExecPolicy,
    secrets: &BTreeMap<String, String>,
    requested: &[String],
) -> RuntimeResult<BTreeMap<String, String>> {
    resolve_exec_secrets_decision("", policy, secrets, requested)
        .map_err(|decision| decision.runtime_error())
}

pub fn resolve_exec_secrets_decision(
    subject: &str,
    policy: &ExecPolicy,
    secrets: &BTreeMap<String, String>,
    requested: &[String],
) -> Result<BTreeMap<String, String>, Box<PolicyDecision>> {
    let mut env = BTreeMap::new();
    for name in requested {
        if !policy.allowed_secrets.iter().any(|allowed| allowed == name) {
            return Err(Box::new(PolicyDecision::denied(
                subject,
                "secret:default",
                "use",
                name,
                PolicyReasonCode::SecretDenied,
                format!("secret `{name}` denied by policy"),
            )));
        }
        let Some(value) = secrets.get(name) else {
            return Err(Box::new(PolicyDecision::denied(
                subject,
                "secret:default",
                "use",
                name,
                PolicyReasonCode::SecretUndefined,
                format!("secret `{name}` is not defined"),
            )));
        };
        env.insert(name.clone(), value.clone());
    }
    Ok(env)
}

pub fn exec_environment(
    policy: &ExecPolicy,
    secrets: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env = if policy.preserve_env {
        std::env::vars().collect()
    } else {
        let mut env = BTreeMap::new();
        for name in &policy.env_allowlist {
            if let Ok(value) = std::env::var(name) {
                env.insert(name.clone(), value);
            }
        }
        env
    };
    env.extend(secrets);
    env
}

fn path_in_policy_scope(path: &str, scope: &str) -> bool {
    let normalized_path = normalize_virtual_path(path);
    let normalized_scope = normalize_virtual_path(scope);
    normalized_path == normalized_scope
        || normalized_path
            .strip_prefix(&normalized_scope)
            .is_some_and(|rest| rest.starts_with('/') || normalized_scope == "/")
}

fn normalize_virtual_path(path: &str) -> String {
    let mut path = format!("/{}", path.trim_start_matches('/'));
    while path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use operon_core::RuntimeErrorKind;

    #[test]
    fn process_capability_ids_are_stable() {
        assert_eq!(PROCESS_CAPABILITY, "process");
        assert_eq!(EXEC_CAPABILITY, "exec");
    }

    #[test]
    fn exec_policy_enforces_cwd_and_timeout() {
        let policy = ExecPolicy {
            allowed_cwds: vec!["/workspace".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            allow_sessions: false,
            preserve_env: false,
            env_allowlist: Vec::new(),
            allowed_secrets: Vec::new(),
        };
        assert!(authorize_exec(&policy, "/workspace/project", Some(30)).is_ok());
        assert_eq!(
            authorize_exec(&policy, "/tmp", Some(30))
                .expect_err("cwd")
                .1,
            "exec cwd denied by policy"
        );
        assert!(authorize_exec(&policy, "/workspace", Some(61))
            .expect_err("timeout")
            .1
            .contains("exceeds policy maximum"));
    }

    #[test]
    fn exec_authorization_decision_names_reason_codes() {
        let policy = ExecPolicy {
            allowed_cwds: vec!["/workspace".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            allow_sessions: false,
            preserve_env: false,
            env_allowlist: Vec::new(),
            allowed_secrets: Vec::new(),
        };

        let allowed = authorize_exec_decision("local-cli", &policy, "/workspace", Some(60));
        assert!(allowed.allowed);
        assert_eq!(allowed.capability_id, "exec:default");
        assert_eq!(allowed.reason_code, operon_core::PolicyReasonCode::Allowed);

        let cwd = authorize_exec_decision("local-cli", &policy, "/tmp", Some(30));
        assert!(!cwd.allowed);
        assert_eq!(
            cwd.reason_code,
            operon_core::PolicyReasonCode::ExecCwdDenied
        );

        let timeout = authorize_exec_decision("local-cli", &policy, "/workspace", Some(61));
        assert!(!timeout.allowed);
        assert_eq!(
            timeout.reason_code,
            operon_core::PolicyReasonCode::ExecTimeoutExceeded
        );
    }

    #[test]
    fn exec_session_policy_requires_session_permission() {
        let mut policy = ExecPolicy {
            allowed_cwds: vec!["/workspace".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            allow_sessions: false,
            preserve_env: false,
            env_allowlist: Vec::new(),
            allowed_secrets: Vec::new(),
        };

        let denied = authorize_exec_session_decision("local-cli", &policy, "/workspace", Some(30));
        assert!(!denied.allowed);
        assert_eq!(
            denied.reason_code,
            operon_core::PolicyReasonCode::ExecSessionDenied
        );

        policy.allow_sessions = true;
        let allowed = authorize_exec_session_decision("local-cli", &policy, "/workspace", Some(30));
        assert!(allowed.allowed);
        assert_eq!(allowed.action, "session");
    }

    #[test]
    fn secret_authorization_decision_names_denied_and_missing_secrets() {
        let policy = ExecPolicy {
            allowed_cwds: vec!["/".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            allow_sessions: false,
            preserve_env: false,
            env_allowlist: Vec::new(),
            allowed_secrets: vec!["TOKEN".to_string()],
        };
        let mut secrets = BTreeMap::new();
        secrets.insert("TOKEN".to_string(), "value".to_string());

        let env =
            resolve_exec_secrets_decision("local-cli", &policy, &secrets, &["TOKEN".to_string()])
                .expect("allowed secret");
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("value"));

        let denied =
            resolve_exec_secrets_decision("local-cli", &policy, &secrets, &["OTHER".to_string()])
                .expect_err("denied secret");
        assert_eq!(denied.capability_id, "secret:default");
        assert_eq!(
            denied.reason_code,
            operon_core::PolicyReasonCode::SecretDenied
        );

        let missing = resolve_exec_secrets_decision(
            "local-cli",
            &policy,
            &BTreeMap::new(),
            &["TOKEN".to_string()],
        )
        .expect_err("missing secret");
        assert_eq!(
            missing.reason_code,
            operon_core::PolicyReasonCode::SecretUndefined
        );
        assert_eq!(missing.runtime_error().0, RuntimeErrorKind::NotFound);
    }

    #[test]
    fn exec_environment_uses_allowlist_and_secrets() {
        let policy = ExecPolicy {
            allowed_cwds: vec!["/".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            allow_sessions: false,
            preserve_env: false,
            env_allowlist: vec!["PATH".to_string()],
            allowed_secrets: Vec::new(),
        };
        let mut secrets = BTreeMap::new();
        secrets.insert("TOKEN".to_string(), "secret".to_string());
        let env = exec_environment(&policy, secrets);
        assert!(env.contains_key("PATH"));
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("secret"));
    }

    #[test]
    fn exec_environment_can_preserve_parent_environment() {
        let policy = ExecPolicy {
            allowed_cwds: vec!["/".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            allow_sessions: false,
            preserve_env: true,
            env_allowlist: Vec::new(),
            allowed_secrets: Vec::new(),
        };
        let mut secrets = BTreeMap::new();
        secrets.insert("TOKEN".to_string(), "secret".to_string());

        let env = exec_environment(&policy, secrets);

        assert!(env.keys().any(|key| key.eq_ignore_ascii_case("PATH")));
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("secret"));
    }
}
