//! System-scoped OpenRC supervision. Never elevates privileges or creates users.
use std::{
    ffi::{CStr, CString},
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::Context;
use operon_config::{resolve_path, validate_private_file_permissions, OperonConfig};
use operon_protocol::runtime::v1::{operon_runtime_client::OperonRuntimeClient, HealthRequest};
use serde::{Deserialize, Serialize};

use crate::daemon_cli::{ServiceCommand, ServiceInstallArgs, ServiceOptions};

const INIT: &str = "/etc/init.d/operond";
const STATE: &str = "/etc/operon/openrc-service.json";
const MARKER: &str = "# Operon-owned OpenRC service v1";
const LOG_DIR: &str = "/var/log/operon";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    format: u32,
    executable: PathBuf,
    config: PathBuf,
    user: String,
    uid: u32,
    gid: u32,
    home: PathBuf,
    stop_timeout_secs: u64,
    shutdown_timeout_secs: u64,
    respawn_delay_secs: u64,
    respawn_max: u32,
    respawn_period_secs: u64,
}

#[derive(Debug)]
struct Identity {
    uid: u32,
    gid: u32,
    home: PathBuf,
}

fn identity(user: &str) -> anyhow::Result<Identity> {
    anyhow::ensure!(
        !user.is_empty()
            && user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "service user must be an existing account name containing only letters, digits, _ or -"
    );
    let name = CString::new(user)?;
    let mut buffer = vec![0u8; 16384];
    loop {
        let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        // getpwnam_r stores all returned pointers in this owned buffer. Read them
        // before the buffer is dropped; do not retain libc's process-global data.
        let error = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                passwd.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if error == libc::ERANGE && buffer.len() < 1024 * 1024 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        anyhow::ensure!(
            error == 0,
            "failed to resolve service user: {}",
            std::io::Error::from_raw_os_error(error)
        );
        anyhow::ensure!(
            !result.is_null(),
            "service account {user} does not exist; create it explicitly before installing"
        );
        let passwd = unsafe { passwd.assume_init() };
        anyhow::ensure!(
            passwd.pw_uid != 0 && passwd.pw_gid != 0,
            "OpenRC daemon identity must have non-root UID and GID"
        );
        let home = PathBuf::from(unsafe { CStr::from_ptr(passwd.pw_dir) }.to_str()?);
        anyhow::ensure!(home.is_absolute(), "service account home must be absolute");
        return Ok(Identity {
            uid: passwd.pw_uid,
            gid: passwd.pw_gid,
            home,
        });
    }
}

fn require_root() -> anyhow::Result<()> {
    anyhow::ensure!(unsafe { libc::geteuid() } == 0,
        "OpenRC is a system service: installation/control requires root; Operon never runs sudo automatically");
    Ok(())
}

fn private_for(path: &Path, uid: u32) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "private file {} must be a regular file, not a symlink",
        path.display()
    );
    validate_private_file_permissions(path)?;
    anyhow::ensure!(
        metadata.uid() == uid,
        "private file {} must be owned by service UID {uid}",
        path.display()
    );
    Ok(())
}

async fn bounded<T>(
    seconds: u64,
    future: impl std::future::Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    if seconds == 0 {
        return future.await;
    }
    tokio::time::timeout(Duration::from_secs(seconds), future).await
        .map_err(|_| anyhow::anyhow!("OpenRC operation timed out after {seconds}s; adjust --timeout-secs/--stop-timeout-secs (0 disables)"))?
}

async fn command(
    program: &str,
    arguments: &[&str],
    seconds: u64,
) -> anyhow::Result<std::process::Output> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    bounded(seconds, async {
        command
            .output()
            .await
            .with_context(|| format!("failed to execute {program}"))
    })
    .await
}

async fn success(program: &str, arguments: &[&str], seconds: u64) -> anyhow::Result<()> {
    let output = command(program, arguments, seconds).await?;
    anyhow::ensure!(
        output.status.success(),
        "{program} {} failed ({}): {}{}",
        arguments.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

async fn check_access(
    registration: &Registration,
    paths: &[(&str, &Path)],
    seconds: u64,
) -> anyhow::Result<()> {
    for (flag, path) in paths {
        let mut child = tokio::process::Command::new("/bin/sh");
        child
            .args(["-c", "test \"$1\" \"$2\"", "operon-access", flag])
            .arg(path)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        // Drop root's supplementary groups before checking actual account access.
        unsafe {
            let uid = registration.uid;
            let gid = registration.gid;
            child.pre_exec(move || {
                if libc::setgroups(0, std::ptr::null()) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::setgid(gid) != 0 || libc::setuid(uid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let status = bounded(seconds, async { Ok(child.status().await?) }).await?;
        anyhow::ensure!(status.success(), "service user {} cannot access {} ({flag}); fix ownership/permissions without making private files public", registration.user, path.display());
    }
    Ok(())
}

async fn validate(registration: &Registration, seconds: u64) -> anyhow::Result<OperonConfig> {
    let account = identity(&registration.user)?;
    anyhow::ensure!(
        account.uid == registration.uid
            && account.gid == registration.gid
            && account.home == registration.home,
        "service identity changed; reinstall the OpenRC service explicitly"
    );
    private_for(&registration.config, account.uid)?;
    let config = OperonConfig::load(&registration.config)?;
    let dir = OperonConfig::config_dir(&registration.config);
    let daemon = config
        .daemon
        .as_ref()
        .context("configuration requires a daemon section")?;
    anyhow::ensure!(
        daemon.auth.token_env.is_none(),
        "OpenRC services require token or token_file, not an inherited token_env"
    );
    anyhow::ensure!(
        daemon.auth.resolve(&dir)?.is_some(),
        "daemon authentication token is required"
    );
    if let Some(token_file) = &daemon.auth.token_file {
        let token = resolve_path(&dir, token_file);
        private_for(&token, account.uid)?;
        check_access(registration, &[("-r", &token)], seconds).await?;
    }
    let workspace = resolve_path(&dir, &daemon.workspace)
        .canonicalize()
        .context("service workspace must already exist")?;
    anyhow::ensure!(workspace.is_dir(), "service workspace must be a directory");
    check_access(
        registration,
        &[
            ("-r", &registration.config),
            ("-x", &registration.executable),
            ("-x", &registration.home),
            ("-w", &workspace),
            ("-x", &workspace),
        ],
        seconds,
    )
    .await?;
    if let Some(store) = crate::store_config::resolve_store_path(&dir, daemon.store.as_deref())? {
        let parent = store.parent().context("store parent is missing")?;
        check_access(registration, &[("-w", parent), ("-x", parent)], seconds).await?;
        if store.exists() {
            private_for(&store, account.uid)?;
            check_access(registration, &[("-w", &store)], seconds).await?;
        }
    }
    if let Some(secrets) = config
        .secrets
        .as_ref()
        .and_then(|secrets| secrets.file.as_ref())
    {
        let secrets = resolve_path(&dir, secrets);
        private_for(&secrets, account.uid)?;
        check_access(registration, &[("-r", &secrets)], seconds).await?;
    }
    Ok(config)
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn text(path: &Path) -> anyhow::Result<&str> {
    path.to_str()
        .context("OpenRC service paths must be valid UTF-8")
}

fn render(registration: &Registration) -> anyhow::Result<String> {
    let executable = quote(&quote(text(&registration.executable)?));
    let config_args = quote(&format!(
        "start --config {} --shutdown-timeout-secs {}",
        quote(text(&registration.config)?),
        registration.shutdown_timeout_secs
    ));
    let directory = quote(&quote(text(&registration.home)?));
    let user = quote(&quote(&registration.user));
    let retry = if registration.stop_timeout_secs == 0 {
        "TERM/forever".to_string()
    } else {
        format!(
            "TERM/{}/KILL/{}",
            registration.stop_timeout_secs, registration.stop_timeout_secs
        )
    };
    Ok(format!(
        r#"#!/sbin/openrc-run
{MARKER}

name="Operon capability daemon"
supervisor=supervise-daemon
command={executable}
command_args={config_args}
directory={directory}
command_user={user}
pidfile=/run/operond-openrc.pid
output_log={LOG_DIR}/stdout.log
error_log={LOG_DIR}/stderr.log
umask=0077
retry={}
respawn_delay={}
respawn_max={}
respawn_period={}
export HOME={}
export USER={}
export LOGNAME={}
export PATH='/usr/libexec/rc/bin:/usr/lib/rc/bin:/lib/rc/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin'

depend() {{
    need localmount
    after bootmisc
}}
"#,
        quote(&retry),
        registration.respawn_delay_secs,
        registration.respawn_max,
        registration.respawn_period_secs,
        quote(text(&registration.home)?),
        quote(&registration.user),
        quote(&registration.user)
    ))
}

fn secure_directory(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        fs::create_dir(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    let metadata = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == 0
            && metadata.mode() & 0o022 == 0,
        "service directory {} must be root-owned and not writable by group/others or symlinked",
        path.display()
    );
    Ok(())
}

fn read_registration() -> anyhow::Result<Registration> {
    let metadata = fs::symlink_metadata(STATE).context("OpenRC service is not installed; run operond service install --backend openrc --config ... --service-user ...")?;
    anyhow::ensure!(
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o077 == 0,
        "OpenRC service registration must be a root-owned private regular file"
    );
    let registration: Registration = serde_json::from_slice(&fs::read(STATE)?)?;
    anyhow::ensure!(
        registration.format == 1,
        "unsupported OpenRC service registration format"
    );
    let metadata = fs::symlink_metadata(INIT)?;
    anyhow::ensure!(
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o022 == 0,
        "OpenRC init script must be a root-owned regular file not writable by group/others"
    );
    anyhow::ensure!(fs::read_to_string(INIT)? == render(&registration)?, "OpenRC service file was modified or is not owned by Operon; refusing to overwrite/control it");
    Ok(registration)
}

fn write_atomic(path: &Path, content: &[u8], mode: u32) -> anyhow::Result<()> {
    let mut temporary = tempfile::NamedTempFile::new_in(path.parent().context("missing parent")?)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(mode))?;
    temporary.write_all(content)?;
    temporary.as_file().sync_all()?;
    if path.exists() {
        temporary.persist(path)?;
    } else {
        temporary.persist_noclobber(path)?;
    }
    Ok(())
}

fn prepare_logs(registration: &Registration) -> anyhow::Result<()> {
    secure_directory(Path::new(LOG_DIR))?;
    for name in ["stdout.log", "stderr.log"] {
        let path = Path::new(LOG_DIR).join(name);
        if !path.exists() {
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            use std::os::fd::AsRawFd;
            anyhow::ensure!(
                unsafe { libc::fchown(file.as_raw_fd(), registration.uid, registration.gid) } == 0,
                "cannot set log ownership"
            );
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        private_for(&path, registration.uid)?;
    }
    Ok(())
}

async fn health(registration: &Registration, seconds: u64) -> anyhow::Result<()> {
    anyhow::ensure!(
        supervised_child(registration, seconds).await?.is_some(),
        "OpenRC has no matching non-root supervised daemon; refusing unrelated endpoint readiness"
    );
    let config = OperonConfig::load(&registration.config)?;
    let daemon = config.daemon.context("daemon section missing")?;
    let token = daemon
        .auth
        .resolve(&OperonConfig::config_dir(&registration.config))?
        .context("daemon token missing")?;
    let mut address = daemon.grpc_listen;
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    bounded(seconds, async {
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{address}"))?
            .connect()
            .await?;
        let mut request = tonic::Request::new(HealthRequest {});
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {token}").parse()?);
        if seconds != 0 {
            request.set_timeout(Duration::from_secs(seconds));
        }
        let response = OperonRuntimeClient::new(channel)
            .health(request)
            .await?
            .into_inner();
        anyhow::ensure!(response.ok, "daemon reported unhealthy status");
        Ok(())
    })
    .await
}

async fn supervised_child(
    registration: &Registration,
    seconds: u64,
) -> anyhow::Result<Option<u32>> {
    let path = Path::new("/run/operond-openrc.pid");
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o022 == 0,
        "OpenRC supervisor PID file must be a root-owned regular file"
    );
    let supervisor: u32 = fs::read_to_string(path)?.trim().parse()?;
    anyhow::ensure!(supervisor > 1, "invalid OpenRC supervisor PID");
    let directory = PathBuf::from(format!("/proc/{supervisor}"));
    let children = match fs::read_to_string(directory.join(format!("task/{supervisor}/children"))) {
        Ok(children) => children,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        directory.metadata()?.uid() == 0,
        "OpenRC supervisor must run as root"
    );
    for child in children.split_whitespace() {
        let pid: u32 = child.parse()?;
        let directory = PathBuf::from(format!("/proc/{pid}"));
        let Ok(metadata) = directory.metadata() else {
            continue;
        };
        if metadata.uid() != registration.uid {
            continue;
        }
        let Ok(command) = fs::read(directory.join("cmdline")) else {
            continue;
        };
        let arguments: Vec<&[u8]> = command.split(|byte| *byte == 0).collect();
        let config = text(&registration.config)?.as_bytes();
        if arguments.get(1) == Some(&b"start".as_slice())
            && arguments
                .windows(2)
                .any(|pair| pair == [b"--config".as_slice(), config])
        {
            // Restricted containers may deny root's /proc/<non-root>/exe read
            // without CAP_SYS_PTRACE. Read it as the daemon's own UID instead;
            // do not demand ptrace privileges or weaken executable matching.
            let mut inspect = tokio::process::Command::new("/usr/bin/readlink");
            inspect
                .arg("-f")
                .arg(directory.join("exe"))
                .stdin(Stdio::null())
                .kill_on_drop(true);
            let uid = registration.uid;
            let gid = registration.gid;
            unsafe {
                inspect.pre_exec(move || {
                    if libc::setgroups(0, std::ptr::null()) != 0
                        || libc::setgid(gid) != 0
                        || libc::setuid(uid) != 0
                    {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let output = bounded(seconds, async { Ok(inspect.output().await?) }).await?;
            if output.status.success()
                && String::from_utf8(output.stdout)?.trim_end() == text(&registration.executable)?
            {
                return Ok(Some(pid));
            }
        }
    }
    Ok(None)
}

async fn wait_ready(registration: &Registration, seconds: u64) -> anyhow::Result<()> {
    bounded(seconds, async {
        loop {
            if health(registration, seconds).await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .context("OpenRC started but daemon did not become healthy; inspect /var/log/operon/stderr.log")
}

async fn stop(options: &ServiceOptions) -> anyhow::Result<()> {
    let status = command("rc-service", &["operond", "status"], options.timeout_secs).await?;
    match status.status.code() {
        Some(3) => return Ok(()),
        Some(0 | 4 | 8 | 16 | 32 | 64) => {}
        _ => anyhow::bail!(
            "could not determine OpenRC status: {}{}",
            String::from_utf8_lossy(&status.stdout),
            String::from_utf8_lossy(&status.stderr)
        ),
    }
    success(
        "rc-service",
        &["operond", "stop"],
        options.stop_timeout_secs,
    )
    .await?;
    let status = command("rc-service", &["operond", "status"], options.timeout_secs).await?;
    anyhow::ensure!(
        status.status.code() == Some(3),
        "OpenRC did not reach stopped state"
    );
    Ok(())
}

async fn install(args: ServiceInstallArgs, options: &ServiceOptions) -> anyhow::Result<()> {
    require_root()?;
    let user = args
        .service_user
        .context("OpenRC installation requires --service-user <existing-non-root-account>")?;
    let account = identity(&user)?;
    let registration = Registration {
        format: 1,
        executable: std::env::current_exe()?.canonicalize()?,
        config: args.config.canonicalize()?,
        user,
        uid: account.uid,
        gid: account.gid,
        home: account.home,
        stop_timeout_secs: options.stop_timeout_secs,
        shutdown_timeout_secs: args.shutdown_timeout_secs,
        respawn_delay_secs: args.respawn_delay_secs,
        respawn_max: args.respawn_max,
        respawn_period_secs: args.respawn_period_secs,
    };
    validate(&registration, options.timeout_secs).await?;
    secure_directory(Path::new("/etc/init.d"))?;
    secure_directory(Path::new("/etc/operon"))?;
    secure_directory(Path::new("/etc/runlevels"))?;
    secure_directory(Path::new("/etc/runlevels/default"))?;
    if let Ok(metadata) = fs::symlink_metadata("/etc/runlevels/default/operond") {
        anyhow::ensure!(
            metadata.file_type().is_symlink()
                && Path::new("/etc/runlevels/default/operond")
                    .canonicalize()
                    .ok()
                    .as_deref()
                    == Some(Path::new(INIT)),
            "conflicting default-runlevel entry exists; review it explicitly before installing"
        );
    }
    let existing = fs::symlink_metadata(INIT).is_ok() || fs::symlink_metadata(STATE).is_ok();
    if existing {
        read_registration()?;
        stop(options).await?;
    }
    prepare_logs(&registration)?;
    write_atomic(Path::new(INIT), render(&registration)?.as_bytes(), 0o755)?;
    write_atomic(
        Path::new(STATE),
        &serde_json::to_vec_pretty(&registration)?,
        0o600,
    )?;
    success(
        "rc-update",
        &["add", "operond", "default"],
        options.timeout_secs,
    )
    .await?;
    result(
        options,
        "installed",
        serde_json::json!({"service_user": registration.user, "boot_runlevel": "default", "started": false}),
    );
    Ok(())
}

fn result(options: &ServiceOptions, operation: &str, details: serde_json::Value) {
    if options.json {
        println!(
            "{}",
            serde_json::json!({"backend": "openrc", "scope": "system", "operation": operation, "details": details})
        );
    } else {
        println!("operond OpenRC system service: {operation} {details}");
    }
}

pub(crate) async fn dispatch(
    command: ServiceCommand,
    options: &ServiceOptions,
) -> anyhow::Result<()> {
    require_root()?;
    if let ServiceCommand::Install(args) = command {
        return install(args, options).await;
    }
    let absent = [INIT, STATE].iter().all(|path| {
        fs::symlink_metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    });
    if absent {
        match command {
            ServiceCommand::Status if options.json => {
                result(
                    options,
                    "status",
                    serde_json::json!({"installed": false, "running": false, "healthy": false, "openrc_status": 3}),
                );
                return Ok(());
            }
            ServiceCommand::Uninstall => {
                require_root()?;
                result(
                    options,
                    "uninstalled",
                    serde_json::json!({"already_absent": true}),
                );
                return Ok(());
            }
            _ => {}
        }
    }
    let registration = read_registration()?;
    match command {
        ServiceCommand::Start => {
            require_root()?;
            validate(&registration, options.timeout_secs).await?;
            success("rc-service", &["operond", "start"], options.timeout_secs).await?;
            wait_ready(&registration, options.timeout_secs).await?;
            result(options, "started", serde_json::json!({"healthy": true}));
        }
        ServiceCommand::Stop => {
            require_root()?;
            stop(options).await?;
            result(options, "stopped", serde_json::json!({}));
        }
        ServiceCommand::Status => {
            let status = command_output_status(options).await?;
            let health_error = if status == 0 {
                health(&registration, options.timeout_secs)
                    .await
                    .err()
                    .map(|error| error.to_string())
            } else {
                Some("OpenRC service is not started".to_string())
            };
            let healthy = health_error.is_none();
            result(
                options,
                "status",
                serde_json::json!({"installed": true, "running": status == 0, "healthy": healthy, "openrc_status": status, "health_error": health_error}),
            );
            if !options.json {
                anyhow::ensure!(
                    status == 0 && healthy,
                    "OpenRC service is not running and healthy"
                );
            }
        }
        ServiceCommand::Uninstall => {
            require_root()?;
            for runlevel in fs::read_dir("/etc/runlevels")? {
                let runlevel = runlevel?;
                if runlevel.file_name() == "default" || !runlevel.file_type()?.is_dir() {
                    continue;
                }
                for entry in fs::read_dir(runlevel.path())? {
                    let entry = entry?;
                    anyhow::ensure!(entry.path().canonicalize().ok().as_deref() != Some(Path::new(INIT)),
                        "another runlevel references operond at {}; remove that entry explicitly before uninstalling; only default is Operon-owned", entry.path().display());
                }
            }
            stop(options).await?;
            success(
                "rc-update",
                &["del", "operond", "default"],
                options.timeout_secs,
            )
            .await?;
            read_registration()?;
            fs::remove_file(INIT)?;
            fs::remove_file(STATE)?;
            result(
                options,
                "uninstalled",
                serde_json::json!({"preserved": ["config", "tokens", "workspace", "store", "logs"]}),
            );
        }
        ServiceCommand::Install(_) => anyhow::bail!("install command was not dispatched"),
        #[cfg(test)]
        ServiceCommand::Run(_) => anyhow::bail!("Windows service run is only available on Windows"),
    }
    Ok(())
}

async fn command_output_status(options: &ServiceOptions) -> anyhow::Result<i32> {
    let status = command("rc-service", &["operond", "status"], options.timeout_secs).await?;
    let code = status
        .status
        .code()
        .context("OpenRC status command was terminated")?;
    anyhow::ensure!(
        [0, 3, 4, 8, 16, 32, 64].contains(&code),
        "OpenRC status command failed: {}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Registration {
        Registration {
            format: 1,
            executable: "/opt/Operon dir/'quoted'/operond".into(),
            config: "/home/service/config '$name'.yaml".into(),
            user: "operon-test".into(),
            uid: 1000,
            gid: 1000,
            home: "/home/operon-test".into(),
            stop_timeout_secs: 37,
            shutdown_timeout_secs: 47,
            respawn_delay_secs: 9,
            respawn_max: 4,
            respawn_period_secs: 82,
        }
    }

    #[test]
    fn render_quotes_both_shell_evaluation_layers() {
        let registration = fixture();
        let script = render(&registration).unwrap();
        assert!(script.contains("supervisor=supervise-daemon"));
        assert!(script.contains("retry='TERM/37/KILL/37'"));
        assert!(script.contains("respawn_delay=9\nrespawn_max=4\nrespawn_period=82"));
        // Source the script without openrc-run, then evaluate precisely the
        // command arguments like supervise-daemon.sh. All metacharacters must
        // remain data through both expansions.
        let mut child = std::process::Command::new("/bin/sh");
        let output = child.arg("-c").arg(format!("{script}\neval 'set -- ' \"$command\" ' -- ' \"$command_args\"; printf '%s\\n' \"$@\""))
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!(
                "{}\n--\nstart\n--config\n{}\n--shutdown-timeout-secs\n47\n",
                registration.executable.display(),
                registration.config.display()
            )
        );
    }

    #[test]
    fn zero_shutdown_deadline_and_invalid_identity() {
        let mut registration = fixture();
        registration.stop_timeout_secs = 0;
        assert!(render(&registration)
            .unwrap()
            .contains("retry='TERM/forever'"));
        assert!(identity("root").is_err());
        assert!(identity("unsafe;name").is_err());
        assert!(identity("operon-no-such-test-account").is_err());
    }
}
