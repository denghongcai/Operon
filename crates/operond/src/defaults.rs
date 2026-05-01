use operon_core::{
    Capability, CapabilityKind, CapabilityList, FsMountPolicy, FsPermissions, FsPolicy, JobPolicy,
    PolicyConfig, ServicePolicy,
};

pub(crate) fn default_policy() -> PolicyConfig {
    PolicyConfig {
        subject: "local-cli".to_string(),
        fs: FsPolicy {
            mounts: vec![FsMountPolicy {
                name: "workspace".to_string(),
                path: "/".to_string(),
                permissions: FsPermissions {
                    read: true,
                    write: true,
                    delete: false,
                },
            }],
        },
        job: JobPolicy {
            allowed_cwds: vec!["/".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 300,
            preserve_env: false,
            env_allowlist: Vec::new(),
            allowed_secrets: Vec::new(),
        },
        service: ServicePolicy::default(),
    }
}

pub(crate) fn default_capabilities(node_id: &str) -> CapabilityList {
    CapabilityList {
        capabilities: vec![
            Capability {
                id: "fs:workspace".to_string(),
                kind: CapabilityKind::Fs,
                node_id: node_id.to_string(),
                name: "workspace".to_string(),
                permissions: vec!["read".to_string(), "write".to_string()],
                description: "Workspace filesystem access".to_string(),
            },
            Capability {
                id: "process:default".to_string(),
                kind: CapabilityKind::Process,
                node_id: node_id.to_string(),
                name: "default".to_string(),
                permissions: vec!["run".to_string()],
                description: "Controlled process execution".to_string(),
            },
            Capability {
                id: "job:default".to_string(),
                kind: CapabilityKind::Job,
                node_id: node_id.to_string(),
                name: "default".to_string(),
                permissions: vec!["run".to_string(), "cancel".to_string(), "logs".to_string()],
                description: "Long-running job execution".to_string(),
            },
            Capability {
                id: "device-info:default".to_string(),
                kind: CapabilityKind::DeviceInfo,
                node_id: node_id.to_string(),
                name: "default".to_string(),
                permissions: vec!["read".to_string()],
                description: "Node OS, architecture, and host metadata".to_string(),
            },
            Capability {
                id: "service:default".to_string(),
                kind: CapabilityKind::Service,
                node_id: node_id.to_string(),
                name: "default".to_string(),
                permissions: vec!["connect".to_string()],
                description: "Service access over an existing private network".to_string(),
            },
        ],
        next_page_token: String::new(),
    }
}
