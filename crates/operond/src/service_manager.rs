use std::path::Path;

#[cfg(not(windows))]
use std::{env, fs, path::PathBuf};

#[cfg(windows)]
use std::{
    env,
    ffi::OsStr,
    fs,
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    ptr,
    sync::{Mutex, OnceLock},
};

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use std::process::Command;

#[cfg(any(test, target_os = "linux"))]
const SYSTEMD_UNIT_NAME: &str = "operond.service";
#[cfg(any(test, target_os = "macos"))]
const LAUNCHD_LABEL: &str = "dev.operon.operond";
#[cfg(any(test, windows))]
const WINDOWS_SERVICE_NAME: &str = "OperonDaemon";

#[cfg(windows)]
static WINDOWS_SERVICE_CONFIG: OnceLock<PathBuf> = OnceLock::new();
#[cfg(windows)]
static WINDOWS_SERVICE_STATUS_HANDLE: OnceLock<usize> = OnceLock::new();
#[cfg(windows)]
static WINDOWS_SERVICE_STOP: OnceLock<Mutex<Option<tokio::sync::oneshot::Sender<()>>>> =
    OnceLock::new();

#[cfg(windows)]
pub(crate) fn install(config: &Path) -> anyhow::Result<()> {
    let paths = install_paths(config)?;
    let args = windows_service_create_args(&paths.executable, &paths.config);
    run_command(
        "sc.exe",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
}

#[cfg(not(windows))]
pub(crate) fn install(config: &Path) -> anyhow::Result<()> {
    let paths = install_paths(config)?;
    platform_install(&paths)
}

pub(crate) fn start() -> anyhow::Result<()> {
    platform_start()
}

pub(crate) fn stop() -> anyhow::Result<()> {
    platform_stop()
}

pub(crate) fn status() -> anyhow::Result<()> {
    platform_status()
}

pub(crate) fn uninstall() -> anyhow::Result<()> {
    platform_uninstall()
}

#[cfg(windows)]
pub(crate) fn run(config: &Path) -> anyhow::Result<()> {
    WINDOWS_SERVICE_CONFIG
        .set(fs::canonicalize(config)?)
        .map_err(|_| anyhow::anyhow!("Windows service config was already initialized"))?;

    let mut service_name = wide_null(WINDOWS_SERVICE_NAME);
    let table = [
        windows_sys::Win32::System::Services::SERVICE_TABLE_ENTRYW {
            lpServiceName: service_name.as_mut_ptr(),
            lpServiceProc: Some(windows_service_main),
        },
        windows_sys::Win32::System::Services::SERVICE_TABLE_ENTRYW {
            lpServiceName: ptr::null_mut(),
            lpServiceProc: None,
        },
    ];

    let ok = unsafe {
        windows_sys::Win32::System::Services::StartServiceCtrlDispatcherW(table.as_ptr())
    };
    if ok == 0 {
        anyhow::bail!(
            "failed to connect to Windows Service Control Manager: {}",
            std::io::Error::last_os_error()
        );
    }

    Ok(())
}

#[cfg(not(windows))]
struct InstallPaths {
    executable: PathBuf,
    config: PathBuf,
}

#[cfg(not(windows))]
fn install_paths(config: &Path) -> anyhow::Result<InstallPaths> {
    if !config.exists() {
        anyhow::bail!("service config file {} does not exist", config.display());
    }

    let executable = env::current_exe()?;
    if !executable.is_absolute() {
        anyhow::bail!(
            "operond executable path {} must be absolute for service install",
            executable.display()
        );
    }

    Ok(InstallPaths {
        executable,
        config: fs::canonicalize(config)?,
    })
}

#[cfg(windows)]
struct InstallPaths {
    executable: PathBuf,
    config: PathBuf,
}

#[cfg(windows)]
fn install_paths(config: &Path) -> anyhow::Result<InstallPaths> {
    if !config.exists() {
        anyhow::bail!("service config file {} does not exist", config.display());
    }

    let executable = env::current_exe()?;
    if !executable.is_absolute() {
        anyhow::bail!(
            "operond executable path {} must be absolute for service install",
            executable.display()
        );
    }

    Ok(InstallPaths {
        executable,
        config: fs::canonicalize(config)?,
    })
}

#[cfg(target_os = "linux")]
fn platform_install(paths: &InstallPaths) -> anyhow::Result<()> {
    let unit_path = systemd_unit_path()?;
    let unit_dir = unit_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("systemd unit path has no parent"))?;
    fs::create_dir_all(unit_dir)?;
    fs::write(
        &unit_path,
        render_systemd_user_unit(&paths.executable, &paths.config),
    )?;
    run_command("systemctl", &["--user", "daemon-reload"])?;
    run_command("systemctl", &["--user", "enable", SYSTEMD_UNIT_NAME])?;
    println!("installed {}", unit_path.display());
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_start() -> anyhow::Result<()> {
    run_command("systemctl", &["--user", "start", SYSTEMD_UNIT_NAME])
}

#[cfg(target_os = "linux")]
fn platform_stop() -> anyhow::Result<()> {
    run_command("systemctl", &["--user", "stop", SYSTEMD_UNIT_NAME])
}

#[cfg(target_os = "linux")]
fn platform_status() -> anyhow::Result<()> {
    run_command(
        "systemctl",
        &["--user", "status", "--no-pager", SYSTEMD_UNIT_NAME],
    )
}

#[cfg(target_os = "linux")]
fn platform_uninstall() -> anyhow::Result<()> {
    let unit_path = systemd_unit_path()?;
    run_command(
        "systemctl",
        &["--user", "disable", "--now", SYSTEMD_UNIT_NAME],
    )?;
    if unit_path.exists() {
        fs::remove_file(&unit_path)?;
    }
    run_command("systemctl", &["--user", "daemon-reload"])?;
    println!("uninstalled {}", unit_path.display());
    Ok(())
}

#[cfg(target_os = "linux")]
fn systemd_unit_path() -> anyhow::Result<PathBuf> {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(config_home)
            .join("systemd")
            .join("user")
            .join(SYSTEMD_UNIT_NAME));
    }

    let home = env::var_os("HOME").ok_or_else(|| {
        anyhow::anyhow!("HOME is required to install a user-level systemd service")
    })?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("systemd")
        .join("user")
        .join(SYSTEMD_UNIT_NAME))
}

#[cfg(target_os = "macos")]
fn platform_install(paths: &InstallPaths) -> anyhow::Result<()> {
    let plist_path = launchd_plist_path()?;
    let plist_dir = plist_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("launchd plist path has no parent"))?;
    fs::create_dir_all(plist_dir)?;
    fs::write(
        &plist_path,
        render_launchd_user_plist(&paths.executable, &paths.config),
    )?;
    run_command(
        "launchctl",
        &[
            "bootstrap",
            &launchd_domain(),
            &plist_path.display().to_string(),
        ],
    )?;
    println!("installed {}", plist_path.display());
    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_start() -> anyhow::Result<()> {
    run_command(
        "launchctl",
        &[
            "kickstart",
            "-k",
            &format!("{}/{}", launchd_domain(), LAUNCHD_LABEL),
        ],
    )
}

#[cfg(target_os = "macos")]
fn platform_stop() -> anyhow::Result<()> {
    run_command(
        "launchctl",
        &[
            "bootout",
            &format!("{}/{}", launchd_domain(), LAUNCHD_LABEL),
        ],
    )
}

#[cfg(target_os = "macos")]
fn platform_status() -> anyhow::Result<()> {
    run_command(
        "launchctl",
        &["print", &format!("{}/{}", launchd_domain(), LAUNCHD_LABEL)],
    )
}

#[cfg(target_os = "macos")]
fn platform_uninstall() -> anyhow::Result<()> {
    let plist_path = launchd_plist_path()?;
    let service = format!("{}/{}", launchd_domain(), LAUNCHD_LABEL);
    run_command("launchctl", &["bootout", &service])?;
    if plist_path.exists() {
        fs::remove_file(&plist_path)?;
    }
    println!("uninstalled {}", plist_path.display());
    Ok(())
}

#[cfg(target_os = "macos")]
fn launchd_plist_path() -> anyhow::Result<PathBuf> {
    let home = env::var_os("HOME")
        .ok_or_else(|| anyhow::anyhow!("HOME is required to install a launchd user agent"))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn launchd_domain() -> String {
    format!("gui/{}", unsafe { libc::getuid() })
}

#[cfg(windows)]
fn platform_start() -> anyhow::Result<()> {
    run_command("sc.exe", &["start", WINDOWS_SERVICE_NAME])
}

#[cfg(windows)]
fn platform_stop() -> anyhow::Result<()> {
    run_command("sc.exe", &["stop", WINDOWS_SERVICE_NAME])
}

#[cfg(windows)]
fn platform_status() -> anyhow::Result<()> {
    run_command("sc.exe", &["query", WINDOWS_SERVICE_NAME])
}

#[cfg(windows)]
fn platform_uninstall() -> anyhow::Result<()> {
    run_command("sc.exe", &["delete", WINDOWS_SERVICE_NAME])
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_install(_paths: &InstallPaths) -> anyhow::Result<()> {
    unsupported_platform()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_start() -> anyhow::Result<()> {
    unsupported_platform()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_stop() -> anyhow::Result<()> {
    unsupported_platform()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_status() -> anyhow::Result<()> {
    unsupported_platform()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_uninstall() -> anyhow::Result<()> {
    unsupported_platform()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn unsupported_platform() -> anyhow::Result<()> {
    anyhow::bail!(
        "operond service management is supported on Linux systemd user services, macOS launchd user agents, and Windows Service hosts"
    )
}

#[cfg(any(test, target_os = "linux"))]
pub(crate) fn render_systemd_user_unit(executable: &Path, config: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=Operon capability daemon\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} start --config {}\n\
         Restart=on-failure\n\
         RestartSec=5s\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        systemd_quote(executable),
        systemd_quote(config)
    )
}

#[cfg(any(test, target_os = "macos"))]
pub(crate) fn render_launchd_user_plist(executable: &Path, config: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         <key>Label</key>\n\
         <string>{}</string>\n\
         <key>ProgramArguments</key>\n\
         <array>\n\
         <string>{}</string>\n\
         <string>start</string>\n\
         <string>--config</string>\n\
         <string>{}</string>\n\
         </array>\n\
         <key>RunAtLoad</key>\n\
         <true/>\n\
         <key>KeepAlive</key>\n\
         <true/>\n\
         <key>StandardOutPath</key>\n\
         <string>{}</string>\n\
         <key>StandardErrorPath</key>\n\
         <string>{}</string>\n\
         </dict>\n\
         </plist>\n",
        xml_escape(LAUNCHD_LABEL),
        xml_escape(&executable.display().to_string()),
        xml_escape(&config.display().to_string()),
        xml_escape("/tmp/operond.stdout.log"),
        xml_escape("/tmp/operond.stderr.log")
    )
}

#[cfg(any(test, windows))]
pub(crate) fn windows_service_create_args(executable: &Path, config: &Path) -> Vec<String> {
    vec![
        "create".to_string(),
        WINDOWS_SERVICE_NAME.to_string(),
        "binPath=".to_string(),
        format!(
            "\"{}\" service run --config \"{}\"",
            executable.display(),
            config.display()
        ),
        "start=".to_string(),
        "auto".to_string(),
        "DisplayName=".to_string(),
        "Operon Daemon".to_string(),
    ]
}

#[cfg(windows)]
unsafe extern "system" fn windows_service_main(_argc: u32, _argv: *mut windows_sys::core::PWSTR) {
    let mut service_name = wide_null(WINDOWS_SERVICE_NAME);
    let handle = unsafe {
        windows_sys::Win32::System::Services::RegisterServiceCtrlHandlerExW(
            service_name.as_mut_ptr(),
            Some(windows_service_handler),
            ptr::null(),
        )
    };
    if handle.is_null() {
        return;
    }

    let _ = WINDOWS_SERVICE_STATUS_HANDLE.set(handle as usize);
    set_windows_service_status(
        windows_sys::Win32::System::Services::SERVICE_START_PENDING,
        0,
        1,
        30_000,
    );

    let Some(config) = WINDOWS_SERVICE_CONFIG.get().cloned() else {
        set_windows_service_status(
            windows_sys::Win32::System::Services::SERVICE_STOPPED,
            1,
            0,
            0,
        );
        return;
    };

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let _ = WINDOWS_SERVICE_STOP.set(Mutex::new(Some(stop_tx)));
    set_windows_service_status(
        windows_sys::Win32::System::Services::SERVICE_RUNNING,
        0,
        0,
        0,
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();
    let result = match runtime {
        Ok(runtime) => runtime.block_on(crate::start_with_shutdown(
            crate::StartArgs {
                config: Some(config),
            },
            async {
                let _ = stop_rx.await;
            },
        )),
        Err(error) => Err(error.into()),
    };

    let exit_code = if result.is_ok() { 0 } else { 1 };
    set_windows_service_status(
        windows_sys::Win32::System::Services::SERVICE_STOPPED,
        exit_code,
        0,
        0,
    );
}

#[cfg(windows)]
unsafe extern "system" fn windows_service_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut core::ffi::c_void,
    _context: *mut core::ffi::c_void,
) -> u32 {
    match control {
        windows_sys::Win32::System::Services::SERVICE_CONTROL_STOP
        | windows_sys::Win32::System::Services::SERVICE_CONTROL_SHUTDOWN => {
            set_windows_service_status(
                windows_sys::Win32::System::Services::SERVICE_STOP_PENDING,
                0,
                1,
                30_000,
            );
            if let Some(sender) = WINDOWS_SERVICE_STOP
                .get()
                .and_then(|sender| sender.lock().ok())
                .and_then(|mut sender| sender.take())
            {
                let _ = sender.send(());
            }
            0
        }
        _ => 0,
    }
}

#[cfg(windows)]
fn set_windows_service_status(
    current_state: u32,
    win32_exit_code: u32,
    checkpoint: u32,
    wait_hint: u32,
) {
    let Some(handle) = WINDOWS_SERVICE_STATUS_HANDLE.get().copied() else {
        return;
    };
    let controls_accepted =
        if current_state == windows_sys::Win32::System::Services::SERVICE_RUNNING {
            windows_sys::Win32::System::Services::SERVICE_ACCEPT_STOP
                | windows_sys::Win32::System::Services::SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        };
    let status = windows_sys::Win32::System::Services::SERVICE_STATUS {
        dwServiceType: windows_sys::Win32::System::Services::SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: current_state,
        dwControlsAccepted: controls_accepted,
        dwWin32ExitCode: win32_exit_code,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: checkpoint,
        dwWaitHint: wait_hint,
    };

    unsafe {
        windows_sys::Win32::System::Services::SetServiceStatus(
            handle as windows_sys::Win32::System::Services::SERVICE_STATUS_HANDLE,
            &status,
        );
    }
}

#[cfg(any(test, target_os = "linux"))]
fn systemd_quote(path: &Path) -> String {
    let value = path.display().to_string();
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        return value;
    }

    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(any(test, target_os = "macos"))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn run_command(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(program).args(args).status()?;
    if status.success() {
        return Ok(());
    }

    anyhow::bail!(
        "supervisor command failed: {} {} exited with {}",
        program,
        args.join(" "),
        status
    )
}

#[cfg(windows)]
fn wide_null(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
