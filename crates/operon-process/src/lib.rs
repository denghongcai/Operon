use std::collections::BTreeMap;

use operon_core::{JobPolicy, PolicyDecision, PolicyReasonCode, RuntimeResult};

pub const PROCESS_CAPABILITY: &str = "process";
pub const JOB_CAPABILITY: &str = "job";

pub fn authorize_job(
    policy: &JobPolicy,
    cwd: &str,
    timeout_secs: Option<u64>,
) -> RuntimeResult<()> {
    let decision = authorize_job_decision("", policy, cwd, timeout_secs);
    if decision.allowed {
        return Ok(());
    }
    Err(decision.runtime_error())
}

pub fn authorize_job_decision(
    subject: &str,
    policy: &JobPolicy,
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
            "job:default",
            "run",
            cwd,
            PolicyReasonCode::JobCwdDenied,
            "job cwd denied by policy",
        );
    }
    if let Some(timeout_secs) = timeout_secs {
        if timeout_secs > policy.max_timeout_secs {
            return PolicyDecision::denied(
                subject,
                "job:default",
                "run",
                cwd,
                PolicyReasonCode::JobTimeoutExceeded,
                format!(
                    "job timeout {timeout_secs}s exceeds policy maximum {}s",
                    policy.max_timeout_secs
                ),
            );
        }
    }
    PolicyDecision::allowed(subject, "job:default", "run", cwd, "allowed")
}

pub fn resolve_job_secrets(
    policy: &JobPolicy,
    secrets: &BTreeMap<String, String>,
    requested: &[String],
) -> RuntimeResult<BTreeMap<String, String>> {
    resolve_job_secrets_decision("", policy, secrets, requested)
        .map_err(|decision| decision.runtime_error())
}

pub fn resolve_job_secrets_decision(
    subject: &str,
    policy: &JobPolicy,
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

pub fn job_environment(
    policy: &JobPolicy,
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
        assert_eq!(JOB_CAPABILITY, "job");
    }

    #[test]
    fn job_policy_enforces_cwd_and_timeout() {
        let policy = JobPolicy {
            allowed_cwds: vec!["/workspace".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            preserve_env: false,
            env_allowlist: Vec::new(),
            allowed_secrets: Vec::new(),
        };
        assert!(authorize_job(&policy, "/workspace/project", Some(30)).is_ok());
        assert_eq!(
            authorize_job(&policy, "/tmp", Some(30)).expect_err("cwd").1,
            "job cwd denied by policy"
        );
        assert!(authorize_job(&policy, "/workspace", Some(61))
            .expect_err("timeout")
            .1
            .contains("exceeds policy maximum"));
    }

    #[test]
    fn job_authorization_decision_names_reason_codes() {
        let policy = JobPolicy {
            allowed_cwds: vec!["/workspace".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            preserve_env: false,
            env_allowlist: Vec::new(),
            allowed_secrets: Vec::new(),
        };

        let allowed = authorize_job_decision("local-cli", &policy, "/workspace", Some(60));
        assert!(allowed.allowed);
        assert_eq!(allowed.capability_id, "job:default");
        assert_eq!(allowed.reason_code, operon_core::PolicyReasonCode::Allowed);

        let cwd = authorize_job_decision("local-cli", &policy, "/tmp", Some(30));
        assert!(!cwd.allowed);
        assert_eq!(cwd.reason_code, operon_core::PolicyReasonCode::JobCwdDenied);

        let timeout = authorize_job_decision("local-cli", &policy, "/workspace", Some(61));
        assert!(!timeout.allowed);
        assert_eq!(
            timeout.reason_code,
            operon_core::PolicyReasonCode::JobTimeoutExceeded
        );
    }

    #[test]
    fn secret_authorization_decision_names_denied_and_missing_secrets() {
        let policy = JobPolicy {
            allowed_cwds: vec!["/".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            preserve_env: false,
            env_allowlist: Vec::new(),
            allowed_secrets: vec!["TOKEN".to_string()],
        };
        let mut secrets = BTreeMap::new();
        secrets.insert("TOKEN".to_string(), "value".to_string());

        let env =
            resolve_job_secrets_decision("local-cli", &policy, &secrets, &["TOKEN".to_string()])
                .expect("allowed secret");
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("value"));

        let denied =
            resolve_job_secrets_decision("local-cli", &policy, &secrets, &["OTHER".to_string()])
                .expect_err("denied secret");
        assert_eq!(denied.capability_id, "secret:default");
        assert_eq!(
            denied.reason_code,
            operon_core::PolicyReasonCode::SecretDenied
        );

        let missing = resolve_job_secrets_decision(
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
    fn job_environment_uses_allowlist_and_secrets() {
        let policy = JobPolicy {
            allowed_cwds: vec!["/".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            preserve_env: false,
            env_allowlist: vec!["PATH".to_string()],
            allowed_secrets: Vec::new(),
        };
        let mut secrets = BTreeMap::new();
        secrets.insert("TOKEN".to_string(), "secret".to_string());
        let env = job_environment(&policy, secrets);
        assert!(env.contains_key("PATH"));
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("secret"));
    }

    #[test]
    fn job_environment_can_preserve_parent_environment() {
        let policy = JobPolicy {
            allowed_cwds: vec!["/".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            preserve_env: true,
            env_allowlist: Vec::new(),
            allowed_secrets: Vec::new(),
        };
        let mut secrets = BTreeMap::new();
        secrets.insert("TOKEN".to_string(), "secret".to_string());

        let env = job_environment(&policy, secrets);

        assert!(env.contains_key("PATH"));
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("secret"));
    }
}
