use crate::{RuntimeErrorKind, ServicePolicy};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyConfig {
    pub subject: String,
    pub fs: FsPolicy,
    pub job: JobPolicy,
    #[serde(default)]
    pub service: ServicePolicy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsPolicy {
    pub mounts: Vec<FsMountPolicy>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsMountPolicy {
    pub name: String,
    pub path: String,
    pub permissions: FsPermissions,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsPermissions {
    pub read: bool,
    pub write: bool,
    pub delete: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobPolicy {
    pub allowed_cwds: Vec<String>,
    pub default_timeout_secs: u64,
    pub max_timeout_secs: u64,
    #[serde(default)]
    pub preserve_env: bool,
    pub env_allowlist: Vec<String>,
    #[serde(default)]
    pub allowed_secrets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PolicyDecision {
    pub subject: String,
    pub capability_id: String,
    pub action: String,
    pub resource: String,
    pub allowed: bool,
    pub reason_code: PolicyReasonCode,
    pub message: String,
}

impl PolicyDecision {
    pub fn allowed(
        subject: impl Into<String>,
        capability_id: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            capability_id: capability_id.into(),
            action: action.into(),
            resource: resource.into(),
            allowed: true,
            reason_code: PolicyReasonCode::Allowed,
            message: message.into(),
        }
    }

    pub fn denied(
        subject: impl Into<String>,
        capability_id: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
        reason_code: PolicyReasonCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            capability_id: capability_id.into(),
            action: action.into(),
            resource: resource.into(),
            allowed: false,
            reason_code,
            message: message.into(),
        }
    }

    pub fn runtime_error(&self) -> (RuntimeErrorKind, String) {
        let kind = match self.reason_code {
            PolicyReasonCode::SecretUndefined => RuntimeErrorKind::NotFound,
            _ => RuntimeErrorKind::Forbidden,
        };
        (kind, self.message.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyReasonCode {
    Allowed,
    FsMountNotAllowed,
    FsPermissionDenied,
    JobCwdDenied,
    JobTimeoutExceeded,
    SecretDenied,
    SecretUndefined,
    ServiceUnknown,
    ServiceActionDenied,
    UnsupportedAction,
}

impl PolicyReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::FsMountNotAllowed => "fs-mount-not-allowed",
            Self::FsPermissionDenied => "fs-permission-denied",
            Self::JobCwdDenied => "job-cwd-denied",
            Self::JobTimeoutExceeded => "job-timeout-exceeded",
            Self::SecretDenied => "secret-denied",
            Self::SecretUndefined => "secret-undefined",
            Self::ServiceUnknown => "service-unknown",
            Self::ServiceActionDenied => "service-action-denied",
            Self::UnsupportedAction => "unsupported-action",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_decision_serializes_stable_reason_code() {
        let decision = PolicyDecision::denied(
            "local-cli",
            "fs:workspace",
            "read",
            "/private",
            PolicyReasonCode::FsMountNotAllowed,
            "path is outside allowed fs mounts",
        );

        let json = serde_json::to_value(&decision).expect("decision json");

        assert_eq!(json["subject"], "local-cli");
        assert_eq!(json["capability_id"], "fs:workspace");
        assert_eq!(json["action"], "read");
        assert_eq!(json["resource"], "/private");
        assert_eq!(json["allowed"], false);
        assert_eq!(json["reason_code"], "fs-mount-not-allowed");
        assert_eq!(json["message"], "path is outside allowed fs mounts");
    }

    #[test]
    fn denied_policy_decision_converts_to_forbidden_runtime_error() {
        let decision = PolicyDecision::denied(
            "local-cli",
            "job:default",
            "run",
            "/tmp",
            PolicyReasonCode::JobCwdDenied,
            "job cwd denied by policy",
        );

        assert_eq!(
            decision.runtime_error(),
            (
                RuntimeErrorKind::Forbidden,
                "job cwd denied by policy".to_string()
            )
        );
    }

    #[test]
    fn allowed_policy_decision_keeps_full_audit_vocabulary() {
        let decision =
            PolicyDecision::allowed("local-cli", "service:web", "forward", "web", "allowed");

        assert!(decision.allowed);
        assert_eq!(decision.reason_code, PolicyReasonCode::Allowed);
        assert_eq!(decision.message, "allowed");
    }

    #[test]
    fn policy_reason_code_has_stable_string_form() {
        assert_eq!(
            PolicyReasonCode::ServiceActionDenied.as_str(),
            "service-action-denied"
        );
        assert_eq!(
            PolicyReasonCode::SecretUndefined.as_str(),
            "secret-undefined"
        );
    }
}
