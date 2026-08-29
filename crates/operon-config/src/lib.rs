use std::{
    collections::BTreeMap,
    env, fmt, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub use operon_core::runtime::NodeEndpoint;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(windows)]
use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER, GENERIC_ALL, GENERIC_READ,
        GENERIC_WRITE, HLOCAL,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, GetExplicitEntriesFromAclW, GetNamedSecurityInfoW,
            GRANT_ACCESS, SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID,
        },
        GetTokenInformation, IsWellKnownSid, TokenUser, WinBuiltinAdministratorsSid,
        WinLocalSystemSid, DACL_SECURITY_INFORMATION, PSID, TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{FILE_ALL_ACCESS, FILE_GENERIC_READ, FILE_GENERIC_WRITE},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

use operon_core::PolicyConfig;

mod warnings;

use warnings::collect_unknown_config_fields;
pub use warnings::ConfigWarning;

#[derive(Debug, Clone)]
pub struct LoadedOperonConfig {
    pub config: OperonConfig,
    pub warnings: Vec<ConfigWarning>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OperonConfig {
    pub version: u32,
    #[serde(default)]
    pub daemon: Option<DaemonConfig>,
    #[serde(default)]
    pub client: ClientConfig,
    #[serde(default)]
    pub policy: Option<PolicyConfig>,
    #[serde(default)]
    pub secrets: Option<SecretsConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonConfig {
    pub node_id: String,
    pub grpc_listen: SocketAddr,
    pub workspace: PathBuf,
    #[serde(default)]
    pub advertise_lan: bool,
    #[serde(default)]
    pub store: Option<PathBuf>,
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ClientConfig {
    #[serde(default)]
    pub nodes: BTreeMap<String, NodeConfig>,
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuthConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SecretsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeConfig {
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "AuthConfig::is_empty")]
    pub auth: AuthConfig,
}

impl fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthConfig")
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("token_file", &self.token_file)
            .field("token_env", &self.token_env)
            .finish()
    }
}

impl OperonConfig {
    pub fn default_path() -> PathBuf {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".operon")
            .join("config.yaml")
    }

    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        let loaded = Self::from_str_with_warnings(&content)?;
        for warning in &loaded.warnings {
            eprintln!("warning: unknown config field `{}` ignored", warning.path);
        }
        Ok(loaded.config)
    }

    pub fn from_str_with_warnings(content: &str) -> anyhow::Result<LoadedOperonConfig> {
        let value: serde_yaml::Value = serde_yaml::from_str(content)?;
        let warnings = collect_unknown_config_fields(&value);
        let config: Self = serde_yaml::from_value(value)?;
        if config.version != 1 {
            anyhow::bail!("unsupported config version `{}`", config.version);
        }
        Ok(LoadedOperonConfig { config, warnings })
    }

    pub fn config_dir(path: impl AsRef<Path>) -> PathBuf {
        path.as_ref()
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    }

    pub fn endpoints(&self, config_dir: &Path) -> anyhow::Result<Vec<NodeEndpoint>> {
        self.client
            .nodes
            .iter()
            .map(|(node_id, node)| node.to_endpoint(node_id, config_dir))
            .collect()
    }

    pub fn endpoint(&self, node_id: &str, config_dir: &Path) -> anyhow::Result<NodeEndpoint> {
        let node = self
            .client
            .nodes
            .get(node_id)
            .ok_or_else(|| anyhow::anyhow!("node `{node_id}` not found in config"))?;
        node.to_endpoint(node_id, config_dir)
    }
}

impl NodeConfig {
    pub fn to_endpoint(&self, node_id: &str, config_dir: &Path) -> anyhow::Result<NodeEndpoint> {
        Ok(NodeEndpoint {
            node_id: node_id.to_string(),
            endpoint: self.endpoint.clone(),
            token: self.auth.resolve(config_dir)?,
        })
    }
}

impl AuthConfig {
    pub fn is_empty(&self) -> bool {
        self.token.is_none() && self.token_file.is_none() && self.token_env.is_none()
    }

    pub fn resolve(&self, config_dir: &Path) -> anyhow::Result<Option<String>> {
        let mut values = Vec::new();
        if let Some(token) = &self.token {
            values.push(token.clone());
        }
        if let Some(path) = &self.token_file {
            let path = resolve_path(config_dir, path);
            values.push(fs::read_to_string(path)?.trim().to_string());
        }
        if let Some(name) = &self.token_env {
            values.push(env::var(name)?);
        }
        match values.len() {
            0 => Ok(None),
            1 => Ok(values.into_iter().next()),
            _ => anyhow::bail!("auth must use only one of token, token_file, or token_env"),
        }
    }
}

pub fn resolve_path(config_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_dir.join(path)
    }
}

pub fn validate_private_file_permissions(path: &Path) -> anyhow::Result<()> {
    validate_private_file_permissions_for_platform(path)
}

#[cfg(unix)]
fn validate_private_file_permissions_for_platform(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        anyhow::bail!("private file `{}` is not a regular file", path.display());
    }
    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        anyhow::bail!(
            "private file `{}` permissions {:o} allow group or other access",
            path.display(),
            mode & 0o777
        );
    }
    Ok(())
}

#[cfg(windows)]
fn validate_private_file_permissions_for_platform(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        anyhow::bail!("private file `{}` is not a regular file", path.display());
    }
    validate_windows_private_file_acl(path)
}

#[cfg(all(not(unix), not(windows)))]
fn validate_private_file_permissions_for_platform(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        anyhow::bail!("private file `{}` is not a regular file", path.display());
    }
    Ok(())
}

#[cfg(any(test, windows))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsAclSummary {
    dacl_present: bool,
    entries: Vec<WindowsAclEntry>,
}

#[cfg(any(test, windows))]
impl WindowsAclSummary {
    #[cfg(test)]
    fn new(entries: Vec<WindowsAclEntry>) -> Self {
        Self {
            dacl_present: true,
            entries,
        }
    }

    #[cfg(windows)]
    fn from_entries(entries: Vec<WindowsAclEntry>) -> Self {
        Self {
            dacl_present: true,
            entries,
        }
    }

    #[cfg(windows)]
    fn missing_dacl() -> Self {
        Self {
            dacl_present: false,
            entries: Vec::new(),
        }
    }

    fn is_private_enough(&self) -> bool {
        self.dacl_present
            && self
                .entries
                .iter()
                .all(|entry| !entry.grants_file_access || entry.trustee.is_private_trustee())
    }
}

#[cfg(any(test, windows))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsAclEntry {
    trustee: WindowsAclTrustee,
    grants_file_access: bool,
}

#[cfg(test)]
impl WindowsAclEntry {
    fn allow(trustee: WindowsAclTrustee) -> Self {
        Self {
            trustee,
            grants_file_access: true,
        }
    }
}

#[cfg(any(test, windows))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum WindowsAclTrustee {
    CurrentUser,
    Administrators,
    LocalSystem,
    Other(String),
}

#[cfg(any(test, windows))]
impl WindowsAclTrustee {
    fn is_private_trustee(&self) -> bool {
        matches!(
            self,
            Self::CurrentUser | Self::Administrators | Self::LocalSystem
        )
    }
}

#[cfg(windows)]
fn validate_windows_private_file_acl(path: &Path) -> anyhow::Result<()> {
    let summary = inspect_windows_private_file_acl(path)?;
    anyhow::ensure!(
        summary.is_private_enough(),
        "private file `{}` Windows ACL grants access outside the current user, Administrators, or SYSTEM",
        path.display()
    );
    Ok(())
}

#[cfg(windows)]
fn inspect_windows_private_file_acl(path: &Path) -> anyhow::Result<WindowsAclSummary> {
    let current_user = current_user_sid()?;
    let mut dacl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let path_wide = path_to_wide(path);
    let status = unsafe {
        GetNamedSecurityInfoW(
            path_wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    anyhow::ensure!(
        status == 0,
        "failed to inspect Windows ACL for {}: {}",
        path.display(),
        status
    );
    let _descriptor = LocalAllocGuard(descriptor as HLOCAL);
    if dacl.is_null() {
        return Ok(WindowsAclSummary::missing_dacl());
    }

    let mut entry_count = 0;
    let mut entries = ptr::null_mut();
    let status = unsafe { GetExplicitEntriesFromAclW(dacl, &mut entry_count, &mut entries) };
    anyhow::ensure!(
        status == 0,
        "failed to enumerate Windows ACL for {}: {}",
        path.display(),
        status
    );
    let _entries = LocalAllocGuard(entries as HLOCAL);

    let entries_slice = unsafe { std::slice::from_raw_parts(entries, entry_count as usize) };
    let mut summary_entries = Vec::with_capacity(entries_slice.len());
    for entry in entries_slice {
        let grants_file_access = matches!(entry.grfAccessMode, GRANT_ACCESS | SET_ACCESS)
            && grants_private_file_access(entry.grfAccessPermissions);
        let trustee = unsafe {
            classify_trustee_sid(
                entry.Trustee.TrusteeForm,
                entry.Trustee.ptstrName,
                &current_user,
            )
        };
        summary_entries.push(WindowsAclEntry {
            trustee,
            grants_file_access,
        });
    }

    Ok(WindowsAclSummary::from_entries(summary_entries))
}

#[cfg(windows)]
fn grants_private_file_access(mask: u32) -> bool {
    let sensitive_bits = FILE_GENERIC_READ
        | FILE_GENERIC_WRITE
        | FILE_ALL_ACCESS
        | GENERIC_READ
        | GENERIC_WRITE
        | GENERIC_ALL;
    mask & sensitive_bits != 0
}

#[cfg(windows)]
unsafe fn classify_trustee_sid(
    trustee_form: i32,
    trustee_name: windows_sys::core::PWSTR,
    current_user: &[u8],
) -> WindowsAclTrustee {
    if trustee_form != TRUSTEE_IS_SID || trustee_name.is_null() {
        return WindowsAclTrustee::Other("unknown-trustee".to_string());
    }
    let sid = trustee_name as PSID;
    if unsafe { windows_sys::Win32::Security::EqualSid(sid, current_user.as_ptr() as PSID) } != 0 {
        return WindowsAclTrustee::CurrentUser;
    }
    if unsafe { IsWellKnownSid(sid, WinBuiltinAdministratorsSid) } != 0 {
        return WindowsAclTrustee::Administrators;
    }
    if unsafe { IsWellKnownSid(sid, WinLocalSystemSid) } != 0 {
        return WindowsAclTrustee::LocalSystem;
    }
    WindowsAclTrustee::Other(
        unsafe { sid_to_string(sid) }.unwrap_or_else(|_| "unknown-sid".to_string()),
    )
}

#[cfg(windows)]
fn current_user_sid() -> anyhow::Result<Vec<u8>> {
    let mut token = ptr::null_mut();
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    anyhow::ensure!(
        ok != 0,
        "failed to open current process token: {}",
        unsafe { GetLastError() }
    );
    let _token = HandleGuard(token);

    let mut needed = 0;
    let ok = unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut needed) };
    if ok == 0 {
        let error = unsafe { GetLastError() };
        anyhow::ensure!(
            error == ERROR_INSUFFICIENT_BUFFER,
            "failed to size current user token information: {error}"
        );
    }
    let mut buffer = vec![0_u8; needed as usize];
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    anyhow::ensure!(
        ok != 0,
        "failed to read current user token information: {}",
        unsafe { GetLastError() }
    );
    let token_user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
    let sid_len = unsafe { windows_sys::Win32::Security::GetLengthSid(token_user.User.Sid) };
    let mut sid = vec![0_u8; sid_len as usize];
    let ok = unsafe {
        windows_sys::Win32::Security::CopySid(sid_len, sid.as_mut_ptr().cast(), token_user.User.Sid)
    };
    anyhow::ensure!(ok != 0, "failed to copy current user SID: {}", unsafe {
        GetLastError()
    });
    Ok(sid)
}

#[cfg(windows)]
unsafe fn sid_to_string(sid: PSID) -> anyhow::Result<String> {
    let mut sid_string = ptr::null_mut();
    let ok = unsafe { ConvertSidToStringSidW(sid, &mut sid_string) };
    anyhow::ensure!(ok != 0, "failed to convert SID to string: {}", unsafe {
        GetLastError()
    });
    let _sid_string = LocalAllocGuard(sid_string as HLOCAL);
    Ok(unsafe { wide_ptr_to_string(sid_string) })
}

#[cfg(windows)]
fn path_to_wide(path: &Path) -> Vec<u16> {
    OsStr::new(path).encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
unsafe fn wide_ptr_to_string(value: windows_sys::core::PWSTR) -> String {
    let mut len = 0;
    while unsafe { *value.add(len) } != 0 {
        len += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(value, len) })
}

#[cfg(windows)]
struct LocalAllocGuard(HLOCAL);

#[cfg(windows)]
impl Drop for LocalAllocGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

#[cfg(windows)]
struct HandleGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_unknown_fields_without_blocking_config_parse() {
        let loaded = OperonConfig::from_str_with_warnings(
            r#"
version: 1
unexpected_root: true
daemon:
  node_id: local
  grpc_listen: 127.0.0.1:7789
  workspace: /workspace
  extra_daemon: true
client:
  nodes:
    gpu:
      endpoint: grpc://100.96.18.20:7789
      provider: tailscale
      auth:
        token: test-token
        ignored_auth: true
      extra_node: true
secrets:
  file: secrets.yaml
  extra_secrets: true
"#,
        )
        .expect("config should parse despite unknown fields");

        let mut warning_paths: Vec<_> = loaded
            .warnings
            .iter()
            .map(|warning| warning.path.as_str())
            .collect();
        warning_paths.sort_unstable();
        assert_eq!(
            warning_paths,
            vec![
                "client.nodes.gpu.auth.ignored_auth",
                "client.nodes.gpu.extra_node",
                "client.nodes.gpu.provider",
                "daemon.extra_daemon",
                "secrets.extra_secrets",
                "unexpected_root",
            ]
        );
        let endpoint = loaded
            .config
            .endpoint("gpu", Path::new("."))
            .expect("gpu endpoint");
        assert_eq!(endpoint.endpoint, "grpc://100.96.18.20:7789");
    }

    #[test]
    fn windows_acl_summary_rejects_public_file_access() {
        let private = WindowsAclSummary::new(vec![
            WindowsAclEntry::allow(WindowsAclTrustee::CurrentUser),
            WindowsAclEntry::allow(WindowsAclTrustee::Administrators),
            WindowsAclEntry::allow(WindowsAclTrustee::LocalSystem),
        ]);
        assert!(private.is_private_enough());

        let public = WindowsAclSummary::new(vec![WindowsAclEntry::allow(
            WindowsAclTrustee::Other("Users".to_string()),
        )]);
        assert!(!public.is_private_enough());
    }

    #[cfg(unix)]
    #[test]
    fn validates_private_file_permissions_on_unix() {
        let base = env::temp_dir().join(format!("operon-config-test-{}", std::process::id()));
        fs::create_dir_all(&base).expect("create temp dir");
        let path = base.join("token");
        fs::write(&path, "token\n").expect("write token");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod private");

        validate_private_file_permissions(&path).expect("private token should validate");

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod broad");
        let error =
            validate_private_file_permissions(&path).expect_err("broad token should be rejected");
        assert!(error.to_string().contains("group or other access"));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&base);
    }

    #[test]
    fn loads_unified_config_with_client_nodes() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:7789
"#,
        )
        .expect("config should parse");

        let endpoint = config
            .endpoint("local", Path::new("."))
            .expect("local endpoint");
        assert_eq!(endpoint.node_id, "local");
        assert_eq!(endpoint.endpoint, "grpc://127.0.0.1:7789");
        assert_eq!(endpoint.token, None);
    }

    #[test]
    fn ignores_legacy_provider_field_in_client_node_config() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
client:
  nodes:
    gpu:
      endpoint: grpc://100.96.18.20:7789
      provider: tailscale
"#,
        )
        .expect("provider should not affect endpoint config");

        let endpoint = config
            .endpoint("gpu", Path::new("."))
            .expect("gpu endpoint");
        assert_eq!(endpoint.node_id, "gpu");
        assert_eq!(endpoint.endpoint, "grpc://100.96.18.20:7789");
    }

    #[test]
    fn returns_endpoints_in_node_id_order() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
client:
  nodes:
    node-b:
      endpoint: grpc://127.0.0.1:17791
    node-a:
      endpoint: grpc://127.0.0.1:17790
"#,
        )
        .expect("config should parse");

        let ids: Vec<_> = config
            .endpoints(Path::new("."))
            .expect("endpoints")
            .into_iter()
            .map(|endpoint| endpoint.node_id)
            .collect();

        assert_eq!(ids, vec!["node-a", "node-b"]);
    }

    #[test]
    fn resolves_inline_node_token() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:7789
      auth:
        token: test-token
"#,
        )
        .expect("config should parse");

        let endpoint = config
            .endpoint("local", Path::new("."))
            .expect("local endpoint");
        assert_eq!(endpoint.token.as_deref(), Some("test-token"));
    }

    #[test]
    fn debug_redacts_inline_tokens() {
        let auth = AuthConfig {
            token: Some("secret-token".to_string()),
            token_file: None,
            token_env: None,
        };
        let endpoint = NodeEndpoint {
            node_id: "local".to_string(),
            endpoint: "grpc://127.0.0.1:7789".to_string(),
            token: Some("secret-token".to_string()),
        };

        let rendered = format!("{auth:?} {endpoint:?}");

        assert!(!rendered.contains("secret-token"));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn omits_empty_auth_when_serializing_node() {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "local".to_string(),
            NodeConfig {
                endpoint: "grpc://127.0.0.1:7789".to_string(),
                auth: AuthConfig::default(),
            },
        );

        let yaml = serde_yaml::to_string(&OperonConfig {
            version: 1,
            daemon: None,
            client: ClientConfig { nodes },
            policy: None,
            secrets: None,
        })
        .expect("config should serialize");

        assert!(!yaml.contains("auth:"));
    }
}
