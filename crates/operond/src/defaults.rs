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

pub(crate) fn capabilities_from_policy(node_id: &str, policy: &PolicyConfig) -> CapabilityList {
    let mut capabilities = Vec::new();

    for mount in &policy.fs.mounts {
        let mut permissions = Vec::new();
        if mount.permissions.read {
            permissions.push("read".to_string());
        }
        if mount.permissions.write {
            permissions.push("write".to_string());
        }
        if mount.permissions.delete {
            permissions.push("delete".to_string());
        }
        capabilities.push(Capability {
            id: format!("fs:{}", mount.name),
            kind: CapabilityKind::Fs,
            node_id: node_id.to_string(),
            name: mount.name.clone(),
            permissions,
            description: format!("Filesystem access to {}", mount.path),
        });
    }

    if !policy.job.allowed_cwds.is_empty() {
        capabilities.push(Capability {
            id: "job:default".to_string(),
            kind: CapabilityKind::Job,
            node_id: node_id.to_string(),
            name: "default".to_string(),
            permissions: vec!["run".to_string(), "cancel".to_string(), "logs".to_string()],
            description: "Policy-scoped job execution".to_string(),
        });
    }

    capabilities.push(Capability {
        id: "device-info:default".to_string(),
        kind: CapabilityKind::DeviceInfo,
        node_id: node_id.to_string(),
        name: "default".to_string(),
        permissions: vec!["read".to_string()],
        description: "Node OS, architecture, and host metadata".to_string(),
    });

    for service in &policy.service.services {
        let mut permissions = Vec::new();
        if service.permissions.check {
            permissions.push("check".to_string());
        }
        if service.permissions.forward {
            permissions.push("forward".to_string());
        }
        capabilities.push(Capability {
            id: format!("service:{}", service.id),
            kind: CapabilityKind::Service,
            node_id: node_id.to_string(),
            name: service.name.clone(),
            permissions,
            description: service.description.clone(),
        });
    }

    CapabilityList {
        capabilities,
        next_page_token: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use operon_core::{ServiceDefinition, ServicePermissions, ServiceProtocol};

    fn empty_policy() -> PolicyConfig {
        PolicyConfig {
            subject: "local-cli".to_string(),
            fs: FsPolicy { mounts: Vec::new() },
            job: JobPolicy {
                allowed_cwds: Vec::new(),
                default_timeout_secs: 30,
                max_timeout_secs: 300,
                preserve_env: false,
                env_allowlist: Vec::new(),
                allowed_secrets: Vec::new(),
            },
            service: ServicePolicy::default(),
        }
    }

    #[test]
    fn policy_capabilities_do_not_advertise_unconfigured_policy_surfaces() {
        let capabilities = capabilities_from_policy("node-a", &empty_policy());
        let ids: Vec<_> = capabilities
            .capabilities
            .iter()
            .map(|capability| capability.id.as_str())
            .collect();

        assert!(!ids.contains(&"fs:workspace"));
        assert!(!ids.contains(&"job:default"));
        assert!(!ids.contains(&"service:default"));
        assert!(ids.contains(&"device-info:default"));
    }

    #[test]
    fn policy_capabilities_reflect_configured_mounts_jobs_and_services() {
        let mut policy = empty_policy();
        policy.fs.mounts.push(FsMountPolicy {
            name: "project".to_string(),
            path: "/project".to_string(),
            permissions: FsPermissions {
                read: true,
                write: false,
                delete: true,
            },
        });
        policy.job.allowed_cwds.push("/project".to_string());
        policy.service.services.push(ServiceDefinition {
            id: "web".to_string(),
            name: "web".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            protocol: ServiceProtocol::Tcp,
            description: "local web".to_string(),
            permissions: ServicePermissions {
                check: true,
                forward: false,
            },
        });

        let capabilities = capabilities_from_policy("node-a", &policy);
        let fs = capabilities
            .capabilities
            .iter()
            .find(|capability| capability.id == "fs:project")
            .expect("fs capability");
        assert_eq!(
            fs.permissions,
            vec!["read".to_string(), "delete".to_string()]
        );
        let job = capabilities
            .capabilities
            .iter()
            .find(|capability| capability.id == "job:default")
            .expect("job capability");
        assert_eq!(
            job.permissions,
            vec!["run".to_string(), "cancel".to_string(), "logs".to_string()]
        );
        let service = capabilities
            .capabilities
            .iter()
            .find(|capability| capability.id == "service:web")
            .expect("service capability");
        assert_eq!(service.permissions, vec!["check".to_string()]);
    }
}
