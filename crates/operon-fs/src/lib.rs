use std::path::{Component, Path, PathBuf};

use operon_core::{
    PolicyConfig, PolicyDecision, PolicyReasonCode, RuntimeErrorKind, RuntimeResult,
};

pub const FILESYSTEM_CAPABILITY: &str = "fs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTraversalHardening {
    CanonicalContainedPath,
}

pub fn workspace_traversal_hardening() -> WorkspaceTraversalHardening {
    WorkspaceTraversalHardening::CanonicalContainedPath
}

pub fn resolve_workspace_path(workspace: &Path, virtual_path: &str) -> RuntimeResult<PathBuf> {
    let trimmed = virtual_path.trim_start_matches('/');
    let mut resolved = workspace.to_path_buf();

    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                return Err((
                    RuntimeErrorKind::Forbidden,
                    "path escapes workspace mount".to_string(),
                ));
            }
        }
    }

    Ok(resolved)
}

pub fn resolve_existing_workspace_path(
    workspace: &Path,
    virtual_path: &str,
) -> RuntimeResult<PathBuf> {
    let raw = resolve_workspace_path(workspace, virtual_path)?;
    contained_canonical_path(workspace, &raw)
}

pub fn resolve_existing_workspace_leaf_path(
    workspace: &Path,
    virtual_path: &str,
) -> RuntimeResult<PathBuf> {
    let raw = resolve_workspace_path(workspace, virtual_path)?;
    ensure_leaf_parent_contained(workspace, &raw)?;
    std::fs::symlink_metadata(&raw)
        .map_err(|error| (RuntimeErrorKind::NotFound, error.to_string()))?;
    Ok(raw)
}

pub fn resolve_write_workspace_path(
    workspace: &Path,
    virtual_path: &str,
) -> RuntimeResult<PathBuf> {
    let raw = resolve_workspace_path(workspace, virtual_path)?;
    match std::fs::symlink_metadata(&raw) {
        Ok(_) => contained_canonical_path(workspace, &raw),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ensure_creatable_path_contained(workspace, &raw)?;
            Ok(raw)
        }
        Err(error) => Err((RuntimeErrorKind::NotFound, error.to_string())),
    }
}

pub fn resolve_create_workspace_path(
    workspace: &Path,
    virtual_path: &str,
) -> RuntimeResult<PathBuf> {
    let raw = resolve_workspace_path(workspace, virtual_path)?;
    ensure_creatable_path_contained(workspace, &raw)?;
    Ok(raw)
}

fn contained_canonical_path(workspace: &Path, raw: &Path) -> RuntimeResult<PathBuf> {
    let workspace = canonicalize_workspace(workspace)?;
    let canonical = raw
        .canonicalize()
        .map_err(|error| (RuntimeErrorKind::NotFound, error.to_string()))?;
    if canonical.starts_with(&workspace) {
        Ok(canonical)
    } else {
        Err((
            RuntimeErrorKind::Forbidden,
            "path resolves outside workspace mount".to_string(),
        ))
    }
}

fn ensure_creatable_path_contained(workspace: &Path, raw: &Path) -> RuntimeResult<()> {
    let workspace = canonicalize_workspace(workspace)?;
    let mut ancestor = raw.parent().unwrap_or(raw);
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or_else(|| {
            (
                RuntimeErrorKind::Forbidden,
                "path has no existing workspace ancestor".to_string(),
            )
        })?;
    }
    let canonical = ancestor
        .canonicalize()
        .map_err(|error| (RuntimeErrorKind::NotFound, error.to_string()))?;
    if canonical.starts_with(&workspace) {
        Ok(())
    } else {
        Err((
            RuntimeErrorKind::Forbidden,
            "path parent resolves outside workspace mount".to_string(),
        ))
    }
}

fn ensure_leaf_parent_contained(workspace: &Path, raw: &Path) -> RuntimeResult<()> {
    let workspace = canonicalize_workspace(workspace)?;
    let parent = raw.parent().ok_or_else(|| {
        (
            RuntimeErrorKind::Forbidden,
            "path has no workspace parent".to_string(),
        )
    })?;
    let canonical = parent
        .canonicalize()
        .map_err(|error| (RuntimeErrorKind::NotFound, error.to_string()))?;
    if canonical.starts_with(&workspace) {
        Ok(())
    } else {
        Err((
            RuntimeErrorKind::Forbidden,
            "path parent resolves outside workspace mount".to_string(),
        ))
    }
}

fn canonicalize_workspace(workspace: &Path) -> RuntimeResult<PathBuf> {
    workspace
        .canonicalize()
        .map_err(|error| (RuntimeErrorKind::NotFound, error.to_string()))
}

pub fn authorize_fs(
    policy: &PolicyConfig,
    operation: &str,
    virtual_path: &str,
) -> RuntimeResult<()> {
    let decision = authorize_fs_decision(policy, operation, virtual_path);
    if decision.allowed {
        return Ok(());
    }
    Err(decision.runtime_error())
}

pub fn authorize_fs_decision(
    policy: &PolicyConfig,
    operation: &str,
    virtual_path: &str,
) -> PolicyDecision {
    let Some(mount) = policy
        .fs
        .mounts
        .iter()
        .find(|mount| path_in_policy_scope(virtual_path, &mount.path))
    else {
        return PolicyDecision::denied(
            &policy.subject,
            "fs",
            operation,
            virtual_path,
            PolicyReasonCode::FsMountNotAllowed,
            "path is outside allowed fs mounts",
        );
    };

    let allowed = match operation {
        "read" => mount.permissions.read,
        "write" => mount.permissions.write,
        "delete" => mount.permissions.delete,
        _ => false,
    };
    if allowed {
        PolicyDecision::allowed(
            &policy.subject,
            format!("fs:{}", mount.name),
            operation,
            virtual_path,
            "allowed",
        )
    } else {
        let reason_code = match operation {
            "read" | "write" | "delete" => PolicyReasonCode::FsPermissionDenied,
            _ => PolicyReasonCode::UnsupportedAction,
        };
        PolicyDecision::denied(
            &policy.subject,
            format!("fs:{}", mount.name),
            operation,
            virtual_path,
            reason_code,
            format!("fs {operation} denied by policy"),
        )
    }
}

pub fn path_in_policy_scope(path: &str, scope: &str) -> bool {
    let normalized_path = normalize_virtual_path(path);
    let normalized_scope = normalize_virtual_path(scope);
    normalized_path == normalized_scope
        || normalized_path
            .strip_prefix(&normalized_scope)
            .is_some_and(|rest| rest.starts_with('/') || normalized_scope == "/")
}

pub fn join_virtual_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), name)
    }
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
    fn filesystem_capability_id_is_stable() {
        assert_eq!(FILESYSTEM_CAPABILITY, "fs");
    }

    #[test]
    fn policy_scope_matches_exact_path_and_children_only() {
        assert!(path_in_policy_scope("/workspace", "/workspace"));
        assert!(path_in_policy_scope("/workspace/file.txt", "/workspace"));
        assert!(!path_in_policy_scope(
            "/workspace-other/file.txt",
            "/workspace"
        ));
    }

    #[test]
    fn fs_authorization_decision_names_capability_and_reason_code() {
        let policy = PolicyConfig {
            subject: "local-cli".to_string(),
            fs: operon_core::FsPolicy {
                mounts: vec![operon_core::FsMountPolicy {
                    name: "workspace".to_string(),
                    path: "/workspace".to_string(),
                    permissions: operon_core::FsPermissions {
                        read: true,
                        write: false,
                        delete: false,
                    },
                }],
            },
            job: operon_core::JobPolicy {
                allowed_cwds: Vec::new(),
                default_timeout_secs: 30,
                max_timeout_secs: 300,
                preserve_env: false,
                env_allowlist: Vec::new(),
                allowed_secrets: Vec::new(),
            },
            service: operon_core::ServicePolicy::default(),
        };

        let allowed = authorize_fs_decision(&policy, "read", "/workspace/file.txt");
        assert!(allowed.allowed);
        assert_eq!(allowed.subject, "local-cli");
        assert_eq!(allowed.capability_id, "fs:workspace");
        assert_eq!(allowed.reason_code, operon_core::PolicyReasonCode::Allowed);

        let denied = authorize_fs_decision(&policy, "write", "/workspace/file.txt");
        assert!(!denied.allowed);
        assert_eq!(denied.capability_id, "fs:workspace");
        assert_eq!(
            denied.reason_code,
            operon_core::PolicyReasonCode::FsPermissionDenied
        );

        let outside = authorize_fs_decision(&policy, "read", "/other/file.txt");
        assert!(!outside.allowed);
        assert_eq!(
            outside.reason_code,
            operon_core::PolicyReasonCode::FsMountNotAllowed
        );
    }

    #[test]
    fn rejects_parent_dir_workspace_escape() {
        let error = resolve_workspace_path(Path::new("/tmp/workspace"), "/../secret")
            .expect_err("parent dir should be rejected");
        assert_eq!(error.0, RuntimeErrorKind::Forbidden);
    }

    #[test]
    fn rejects_symlink_escape_for_existing_path() {
        let base = std::env::temp_dir().join(format!(
            "operon-fs-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let workspace = base.join("workspace");
        let outside = base.join("outside");
        std::fs::create_dir_all(&workspace).expect("workspace");
        std::fs::create_dir_all(&outside).expect("outside");
        std::fs::write(outside.join("secret.txt"), "secret").expect("secret");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, workspace.join("link")).expect("symlink");

        #[cfg(unix)]
        {
            let error = resolve_existing_workspace_path(&workspace, "/link/secret.txt")
                .expect_err("symlink escape should be rejected");
            assert_eq!(error.0, RuntimeErrorKind::Forbidden);
        }

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn existing_leaf_symlink_resolves_to_link_itself() {
        let base = std::env::temp_dir().join(format!(
            "operon-fs-leaf-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let workspace = base.join("workspace");
        let outside = base.join("outside");
        std::fs::create_dir_all(&workspace).expect("workspace");
        std::fs::create_dir_all(&outside).expect("outside");
        std::fs::write(outside.join("secret.txt"), "secret").expect("secret");

        #[cfg(unix)]
        {
            let link = workspace.join("link");
            std::os::unix::fs::symlink(outside.join("secret.txt"), &link).expect("symlink");
            let resolved = resolve_existing_workspace_leaf_path(&workspace, "/link")
                .expect("leaf symlink should resolve to link path");
            assert_eq!(resolved, link);
        }

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn traversal_hardening_strategy_is_explicit() {
        assert_eq!(
            workspace_traversal_hardening(),
            WorkspaceTraversalHardening::CanonicalContainedPath
        );
    }

    #[test]
    fn rejects_creating_path_below_symlink_parent_escape() {
        let base = std::env::temp_dir().join(format!(
            "operon-fs-create-symlink-parent-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let workspace = base.join("workspace");
        let outside = base.join("outside");
        std::fs::create_dir_all(&workspace).expect("workspace");
        std::fs::create_dir_all(&outside).expect("outside");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, workspace.join("link")).expect("symlink");
            let error = resolve_create_workspace_path(&workspace, "/link/new.txt")
                .expect_err("symlink parent escape should be rejected");
            assert_eq!(error.0, RuntimeErrorKind::Forbidden);
        }

        let _ = std::fs::remove_dir_all(base);
    }
}
