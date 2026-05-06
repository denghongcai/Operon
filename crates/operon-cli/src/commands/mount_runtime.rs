#[derive(Debug, serde::Serialize)]
pub(crate) struct MountRuntimeReport {
    pub(crate) adapter: &'static str,
    pub(crate) status: &'static str,
    pub(crate) ready: bool,
    pub(crate) hint: &'static str,
}

pub(crate) fn report() -> MountRuntimeReport {
    let runtime = runtime_diagnostic();
    MountRuntimeReport {
        adapter: adapter_diagnostic(),
        status: runtime.status,
        ready: runtime.ready,
        hint: runtime.hint,
    }
}

pub(crate) fn setup_hint() -> &'static str {
    report().hint
}

pub(crate) fn adapter_name() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "linux-fuse"
    }
    #[cfg(target_os = "macos")]
    {
        "macos-fuse-t"
    }
    #[cfg(windows)]
    {
        "windows-winfsp"
    }
    #[cfg(all(not(target_os = "linux"), not(target_os = "macos"), not(windows)))]
    {
        "unsupported"
    }
}

#[cfg(target_os = "linux")]
fn adapter_diagnostic() -> &'static str {
    "linux-fuse-supported"
}

#[cfg(target_os = "macos")]
fn adapter_diagnostic() -> &'static str {
    "macos-fuse-t-supported-runtime-required"
}

#[cfg(windows)]
fn adapter_diagnostic() -> &'static str {
    "windows-winfsp-supported-runtime-required"
}

#[cfg(all(not(target_os = "linux"), not(target_os = "macos"), not(windows)))]
fn adapter_diagnostic() -> &'static str {
    "mount-adapter-unsupported-platform"
}

struct RuntimeDiagnostic {
    status: &'static str,
    ready: bool,
    hint: &'static str,
}

#[cfg(target_os = "linux")]
fn runtime_diagnostic() -> RuntimeDiagnostic {
    if !std::path::Path::new("/dev/fuse").exists() {
        return RuntimeDiagnostic {
            status: "linux-fuse-runtime-missing",
            ready: false,
            hint: "install/configure FUSE and ensure /dev/fuse is available to the user running operon mount",
        };
    }
    if !command_exists("fusermount3") && !command_exists("fusermount") {
        return RuntimeDiagnostic {
            status: "linux-fuse-helper-missing",
            ready: false,
            hint: "install fuse3 or fuse so fusermount3/fusermount is available on PATH",
        };
    }
    RuntimeDiagnostic {
        status: "linux-fuse-runtime-found",
        ready: true,
        hint: "Linux live mounts require host FUSE permissions and fusermount access",
    }
}

#[cfg(target_os = "macos")]
fn runtime_diagnostic() -> RuntimeDiagnostic {
    if macos_fuse_t_library_exists() || pkg_config_resolves("fuse") {
        RuntimeDiagnostic {
            status: "macos-fuse-t-runtime-found",
            ready: true,
            hint: "macOS live mounts require FUSE-T; use OPERON_MOUNT_MACOS_BACKEND=nfs by default or fskit for /Volumes mount points",
        }
    } else {
        RuntimeDiagnostic {
            status: "macos-fuse-t-runtime-missing",
            ready: false,
            hint: "install FUSE-T with `brew install macos-fuse-t/homebrew-cask/fuse-t` before running operon mount",
        }
    }
}

#[cfg(windows)]
fn runtime_diagnostic() -> RuntimeDiagnostic {
    if windows_winfsp_library_exists() {
        RuntimeDiagnostic {
            status: "windows-winfsp-runtime-found",
            ready: true,
            hint: "Windows live mounts require the WinFsp runtime and service to be installed",
        }
    } else {
        RuntimeDiagnostic {
            status: "windows-winfsp-runtime-missing",
            ready: false,
            hint: "install WinFsp before running operon mount; CI uses `choco install winfsp -y`",
        }
    }
}

#[cfg(all(not(target_os = "linux"), not(target_os = "macos"), not(windows)))]
fn runtime_diagnostic() -> RuntimeDiagnostic {
    RuntimeDiagnostic {
        status: "mount-runtime-unsupported-platform",
        ready: false,
        hint: "live mount adapters are supported only on Linux, macOS, and Windows",
    }
}

#[cfg(target_os = "linux")]
fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|path| path.join(name).is_file()))
}

#[cfg(target_os = "macos")]
fn macos_fuse_t_library_exists() -> bool {
    [
        "/usr/local/lib/libfuse-t.dylib",
        "/opt/homebrew/lib/libfuse-t.dylib",
        "/Library/Application Support/fuse-t/lib/libfuse-t.dylib",
    ]
    .into_iter()
    .any(|path| std::path::Path::new(path).is_file())
}

#[cfg(target_os = "macos")]
fn pkg_config_resolves(package: &str) -> bool {
    std::process::Command::new("pkg-config")
        .arg("--modversion")
        .arg(package)
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(windows)]
fn windows_winfsp_library_exists() -> bool {
    let mut candidates = Vec::new();
    if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
        candidates.push(
            std::path::PathBuf::from(program_files_x86)
                .join("WinFsp")
                .join("bin")
                .join("winfsp-x64.dll"),
        );
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(
            std::path::PathBuf::from(program_files)
                .join("WinFsp")
                .join("bin")
                .join("winfsp-x64.dll"),
        );
    }
    candidates.into_iter().any(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_runtime_report_has_operator_fields() {
        let report = report();
        assert!(!report.adapter.is_empty());
        assert!(!report.status.is_empty());
        assert!(!report.hint.is_empty());
    }
}
