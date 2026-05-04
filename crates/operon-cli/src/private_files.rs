use std::{fs, path::Path};

#[cfg(unix)]
use std::{
    fs::OpenOptions,
    io::Write as _,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};

#[cfg(not(unix))]
use std::io::Write as _;

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
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetExplicitEntriesFromAclW, GetNamedSecurityInfoW, GRANT_ACCESS, SET_ACCESS,
            SE_FILE_OBJECT, TRUSTEE_IS_SID,
        },
        GetTokenInformation, IsWellKnownSid, TokenUser, WinBuiltinAdministratorsSid,
        WinLocalSystemSid, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID, TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{FILE_ALL_ACCESS, FILE_GENERIC_READ, FILE_GENERIC_WRITE},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub(crate) fn generate_token() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)?;
    let mut token = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        token.push(HEX[(byte >> 4) as usize] as char);
        token.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(token)
}

pub(crate) fn private_file_security_model() -> &'static str {
    private_file_security_model_for_platform()
}

#[cfg(unix)]
fn private_file_security_model_for_platform() -> &'static str {
    "unix-owner-only-mode"
}

#[cfg(windows)]
fn private_file_security_model_for_platform() -> &'static str {
    "windows-acl-verified"
}

#[cfg(all(not(unix), not(windows)))]
fn private_file_security_model_for_platform() -> &'static str {
    "basic-create-warning"
}

#[cfg(unix)]
pub(crate) fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    validate_private_file_target(path)?;
    let mut handle = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    handle.write_all(content.as_bytes())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
pub(crate) fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    let mut handle = fs::File::create(path)?;
    handle.write_all(content.as_bytes())?;
    Ok(())
}

#[cfg(windows)]
pub(crate) fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    validate_private_file_target(path)?;
    let mut handle = fs::File::create(path)?;
    handle.write_all(content.as_bytes())?;
    drop(handle);
    protect_windows_private_file_acl(path)?;
    validate_windows_private_file_acl(path)?;
    Ok(())
}

#[cfg(unix)]
fn validate_private_file_target(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        !metadata.file_type().is_symlink(),
        "refusing to write private file {} because it is a symlink",
        path.display()
    );
    let mode = metadata.permissions().mode() & 0o777;
    anyhow::ensure!(
        mode & 0o077 == 0,
        "refusing to write private file {} with permissions {:03o}; set permissions to 600 first",
        path.display(),
        mode
    );
    Ok(())
}

#[cfg(windows)]
fn validate_private_file_target(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        !metadata.file_type().is_symlink(),
        "refusing to write private file {} because it is a symlink",
        path.display()
    );
    validate_windows_private_file_acl(path)
}

#[cfg(any(test, windows))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsAclSummary {
    dacl_present: bool,
    entries: Vec<WindowsAclEntry>,
}

#[cfg(any(test, windows))]
impl WindowsAclSummary {
    fn new(entries: Vec<WindowsAclEntry>) -> Self {
        Self {
            dacl_present: true,
            entries,
        }
    }

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

#[cfg(any(test, windows))]
impl WindowsAclEntry {
    #[cfg(test)]
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
        "refusing to write private file {} because its Windows ACL grants access outside the current user, Administrators, or SYSTEM",
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

    Ok(WindowsAclSummary::new(summary_entries))
}

#[cfg(windows)]
fn protect_windows_private_file_acl(path: &Path) -> anyhow::Result<()> {
    let user_sid = current_user_sid_string()?;
    let sddl = format!("D:P(A;;FA;;;{user_sid})(A;;FA;;;SY)(A;;FA;;;BA)");
    let sddl_wide = str_to_wide(&sddl);
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl_wide.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    };
    anyhow::ensure!(
        ok != 0,
        "failed to build Windows private-file ACL security descriptor: {}",
        unsafe { GetLastError() }
    );
    let _descriptor = LocalAllocGuard(descriptor as HLOCAL);

    let path_wide = path_to_wide(path);
    let ok = unsafe {
        windows_sys::Win32::Security::SetFileSecurityW(
            path_wide.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    anyhow::ensure!(
        ok != 0,
        "failed to apply Windows private-file ACL to {}: {}",
        path.display(),
        unsafe { GetLastError() }
    );
    Ok(())
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
    if EqualSidBytes(sid, current_user) {
        return WindowsAclTrustee::CurrentUser;
    }
    if IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0 {
        return WindowsAclTrustee::Administrators;
    }
    if IsWellKnownSid(sid, WinLocalSystemSid) != 0 {
        return WindowsAclTrustee::LocalSystem;
    }
    WindowsAclTrustee::Other(sid_to_string(sid).unwrap_or_else(|_| "unknown-sid".to_string()))
}

#[cfg(windows)]
#[allow(non_snake_case)]
unsafe fn EqualSidBytes(sid: PSID, expected: &[u8]) -> bool {
    windows_sys::Win32::Security::EqualSid(sid, expected.as_ptr() as PSID) != 0
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
fn current_user_sid_string() -> anyhow::Result<String> {
    let sid = current_user_sid()?;
    unsafe { sid_to_string(sid.as_ptr() as PSID) }
}

#[cfg(windows)]
unsafe fn sid_to_string(sid: PSID) -> anyhow::Result<String> {
    let mut sid_string = ptr::null_mut();
    let ok = ConvertSidToStringSidW(sid, &mut sid_string);
    anyhow::ensure!(
        ok != 0,
        "failed to convert SID to string: {}",
        GetLastError()
    );
    let _sid_string = LocalAllocGuard(sid_string as HLOCAL);
    Ok(wide_ptr_to_string(sid_string))
}

#[cfg(windows)]
fn path_to_wide(path: &Path) -> Vec<u16> {
    OsStr::new(path).encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
fn str_to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
unsafe fn wide_ptr_to_string(value: windows_sys::core::PWSTR) -> String {
    let mut len = 0;
    while *value.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(value, len))
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

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn generated_token_is_hex_encoded() {
        let token = generate_token().expect("token");
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|value| value.is_ascii_hexdigit()));
    }

    #[test]
    fn private_file_security_model_is_platform_specific() {
        #[cfg(unix)]
        assert_eq!(private_file_security_model(), "unix-owner-only-mode");

        #[cfg(windows)]
        assert_eq!(private_file_security_model(), "windows-acl-verified");

        #[cfg(all(not(unix), not(windows)))]
        assert_eq!(private_file_security_model(), "basic-create-warning");
    }

    #[test]
    fn windows_private_file_acl_model_allows_current_user_admins_and_system() {
        let acl = WindowsAclSummary::new(vec![
            WindowsAclEntry::allow(WindowsAclTrustee::CurrentUser),
            WindowsAclEntry::allow(WindowsAclTrustee::Administrators),
            WindowsAclEntry::allow(WindowsAclTrustee::LocalSystem),
        ]);

        assert!(acl.is_private_enough());
    }

    #[test]
    fn windows_private_file_acl_model_rejects_other_trustee_with_access() {
        let acl = WindowsAclSummary::new(vec![WindowsAclEntry::allow(WindowsAclTrustee::Other(
            "Everyone".to_string(),
        ))]);

        assert!(!acl.is_private_enough());
    }

    #[test]
    fn windows_private_file_acl_model_rejects_missing_dacl() {
        assert!(!WindowsAclSummary::missing_dacl().is_private_enough());
    }

    #[cfg(unix)]
    #[test]
    fn private_file_refuses_broad_existing_permissions() {
        let base = tempfile::tempdir().expect("temp dir");
        let path = base.path().join("token");
        fs::write(&path, "old\n").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

        let error =
            write_private_file(&path, "new\n").expect_err("broad token file should be rejected");

        assert!(error.to_string().contains("refusing to write private file"));
    }

    #[cfg(unix)]
    #[test]
    fn private_file_is_written_with_owner_only_permissions() {
        let base = tempfile::tempdir().expect("temp dir");
        let path = base.path().join("token");

        write_private_file(&path, "new\n").expect("write private file");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(windows)]
    #[test]
    fn windows_private_file_is_written_with_verified_acl() {
        let base = tempfile::tempdir().expect("temp dir");
        let path = base.path().join("token");

        write_private_file(&path, "new\n").expect("write private file");

        validate_windows_private_file_acl(&path).expect("private ACL should validate");
    }
}
