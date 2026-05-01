use std::collections::BTreeMap;

use operon_core::{JobPolicy, RuntimeErrorKind, RuntimeResult};

pub const PROCESS_CAPABILITY: &str = "process";
pub const JOB_CAPABILITY: &str = "job";

pub fn authorize_job(
    policy: &JobPolicy,
    cwd: &str,
    timeout_secs: Option<u64>,
) -> RuntimeResult<()> {
    if !policy
        .allowed_cwds
        .iter()
        .any(|allowed| path_in_policy_scope(cwd, allowed))
    {
        return Err((
            RuntimeErrorKind::Forbidden,
            "job cwd denied by policy".to_string(),
        ));
    }
    if let Some(timeout_secs) = timeout_secs {
        if timeout_secs > policy.max_timeout_secs {
            return Err((
                RuntimeErrorKind::Forbidden,
                format!(
                    "job timeout {timeout_secs}s exceeds policy maximum {}s",
                    policy.max_timeout_secs
                ),
            ));
        }
    }
    Ok(())
}

pub fn resolve_job_secrets(
    policy: &JobPolicy,
    secrets: &BTreeMap<String, String>,
    requested: &[String],
) -> RuntimeResult<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    for name in requested {
        if !policy.allowed_secrets.iter().any(|allowed| allowed == name) {
            return Err((
                RuntimeErrorKind::Forbidden,
                format!("secret `{name}` denied by policy"),
            ));
        }
        let Some(value) = secrets.get(name) else {
            return Err((
                RuntimeErrorKind::NotFound,
                format!("secret `{name}` is not defined"),
            ));
        };
        env.insert(name.clone(), value.clone());
    }
    Ok(env)
}

pub fn job_environment(
    policy: &JobPolicy,
    secrets: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    for name in &policy.env_allowlist {
        if let Ok(value) = std::env::var(name) {
            env.insert(name.clone(), value);
        }
    }
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
    fn job_environment_uses_allowlist_and_secrets() {
        let policy = JobPolicy {
            allowed_cwds: vec!["/".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 60,
            env_allowlist: vec!["PATH".to_string()],
            allowed_secrets: Vec::new(),
        };
        let mut secrets = BTreeMap::new();
        secrets.insert("TOKEN".to_string(), "secret".to_string());
        let env = job_environment(&policy, secrets);
        assert!(env.contains_key("PATH"));
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("secret"));
    }
}
