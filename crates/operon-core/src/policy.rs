use crate::ServicePolicy;

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
