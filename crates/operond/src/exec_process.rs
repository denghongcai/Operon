use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use operon_core::{ExecLog, ExecRecord};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Child,
    sync::mpsc,
    task::JoinHandle,
};

use crate::{
    exec_runtime::append_exec_log,
    state::{ExecLogBuffer, ExecLogSender},
};

pub(crate) async fn terminate_child(child: &mut Child, child_group: &ExecChildGroup) {
    #[cfg(unix)]
    {
        let _ = child_group;
        terminate_child_process_group(child).await
    }

    #[cfg(windows)]
    {
        if !child_group.terminate() {
            terminate_direct_child(child).await;
        }
    }

    #[cfg(all(not(unix), not(windows)))]
    {
        let _ = child_group;
        terminate_direct_child(child).await
    }
}

#[cfg(test)]
pub(crate) fn exec_cancellation_guarantee() -> &'static str {
    exec_cancellation_guarantee_for_platform()
}

#[cfg(all(test, unix))]
fn exec_cancellation_guarantee_for_platform() -> &'static str {
    "process-group"
}

#[cfg(all(test, windows))]
fn exec_cancellation_guarantee_for_platform() -> &'static str {
    "job-object-process-tree"
}

#[cfg(all(test, not(unix), not(windows)))]
fn exec_cancellation_guarantee_for_platform() -> &'static str {
    "direct-child-best-effort"
}

pub(crate) struct ExecChildGroup {
    #[cfg(windows)]
    job: Option<WindowsJobObject>,
}

impl ExecChildGroup {
    pub(crate) fn attach(child: &Child) -> Self {
        #[cfg(windows)]
        {
            Self {
                job: WindowsJobObject::assign_child(child),
            }
        }

        #[cfg(not(windows))]
        {
            let _ = child;
            Self {}
        }
    }
}

#[cfg(windows)]
impl ExecChildGroup {
    fn terminate(&self) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        job.terminate()
    }
}

#[cfg(not(windows))]
impl ExecChildGroup {}

#[cfg(windows)]
struct WindowsJobObject {
    handle: usize,
}

#[cfg(windows)]
impl WindowsJobObject {
    fn assign_child(child: &Child) -> Option<Self> {
        use std::ptr;
        use windows_sys::Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW},
        };

        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            tracing::warn!(
                "failed to create Windows Job Object for exec process tree: {}",
                std::io::Error::last_os_error()
            );
            return None;
        }

        let Some(process) = child.raw_handle() else {
            unsafe {
                CloseHandle(handle);
            }
            tracing::warn!(
                "failed to assign exec process to Windows Job Object: missing child process handle"
            );
            return None;
        };
        let process = process as HANDLE;
        let assigned = unsafe { AssignProcessToJobObject(handle, process) };
        if assigned == 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                CloseHandle(handle);
            }
            tracing::warn!("failed to assign exec process to Windows Job Object: {error}");
            return None;
        }

        Some(Self {
            handle: handle as usize,
        })
    }

    fn terminate(&self) -> bool {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        let terminated = unsafe { TerminateJobObject(self.handle(), 1) };
        if terminated == 0 {
            tracing::warn!(
                "failed to terminate Windows exec Job Object: {}",
                std::io::Error::last_os_error()
            );
            return false;
        }
        true
    }

    fn handle(&self) -> windows_sys::Win32::Foundation::HANDLE {
        self.handle as windows_sys::Win32::Foundation::HANDLE
    }
}

#[cfg(windows)]
impl Drop for WindowsJobObject {
    fn drop(&mut self) {
        let closed = unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle()) };
        if closed == 0 {
            tracing::warn!(
                "failed to close Windows exec Job Object handle: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}

#[cfg(unix)]
pub(crate) async fn terminate_child_process_group(child: &mut Child) {
    let Some(pid) = child.id().map(|pid| pid as libc::pid_t) else {
        if let Err(error) = child.wait().await {
            tracing::warn!("failed to wait for finished exec process: {error}");
        }
        return;
    };

    signal_process_group(pid, libc::SIGTERM);
    match tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await {
        Ok(Ok(_)) => return,
        Ok(Err(error)) => {
            tracing::warn!("failed to wait for terminated exec process group: {error}");
            return;
        }
        Err(_) => {}
    }

    signal_process_group(pid, libc::SIGKILL);
    if let Err(error) = child.wait().await {
        tracing::warn!("failed to wait for killed exec process group: {error}");
    }
}

#[cfg(unix)]
fn signal_process_group(pgid: libc::pid_t, signal: libc::c_int) {
    let result = unsafe { libc::kill(-pgid, signal) };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            tracing::warn!("failed to signal exec process group {pgid}: {error}");
        }
    }
}

#[cfg(not(unix))]
pub(crate) async fn terminate_direct_child(child: &mut Child) {
    if let Err(error) = child.start_kill() {
        tracing::warn!("failed to kill exec process: {error}");
    }
    if let Err(error) = child.wait().await {
        tracing::warn!("failed to wait for killed exec process: {error}");
    }
}

pub(crate) async fn wait_for_capture_tasks(capture_tasks: Vec<JoinHandle<()>>) {
    for task in capture_tasks {
        if let Err(error) = task.await {
            tracing::warn!("exec stream capture task failed: {error}");
        }
    }
}

pub(crate) async fn capture_exec_stream<R>(
    execs: Arc<Mutex<BTreeMap<String, ExecRecord>>>,
    logs: Arc<Mutex<BTreeMap<String, ExecLogBuffer>>>,
    log_events: Arc<Mutex<BTreeMap<String, ExecLogSender>>>,
    store_writer: operon_store::StoreWriter,
    exec_id: String,
    stream: &'static str,
    mut reader: R,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(count) => append_exec_log(
                &execs,
                &logs,
                &log_events,
                &store_writer,
                &exec_id,
                ExecLog {
                    stream: stream.to_string(),
                    data: buffer[..count].to_vec(),
                    sequence: 0,
                },
            ),
            Err(error) => {
                append_exec_log(
                    &execs,
                    &logs,
                    &log_events,
                    &store_writer,
                    &exec_id,
                    ExecLog {
                        stream: "stderr".to_string(),
                        data: format!("failed to read {stream}: {error}").into_bytes(),
                        sequence: 0,
                    },
                );
                break;
            }
        }
    }
}

pub(crate) async fn pump_exec_stdin(
    mut receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    mut stdin: tokio::process::ChildStdin,
) {
    while let Some(chunk) = receiver.recv().await {
        if stdin.write_all(&chunk).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_cancellation_guarantee_matches_platform() {
        #[cfg(unix)]
        assert_eq!(exec_cancellation_guarantee(), "process-group");

        #[cfg(windows)]
        assert_eq!(exec_cancellation_guarantee(), "job-object-process-tree");

        #[cfg(all(not(unix), not(windows)))]
        assert_eq!(exec_cancellation_guarantee(), "direct-child-best-effort");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_job_object_cancellation_terminates_descendant_process() {
        use std::{fs, process::Stdio, time::Duration};

        use tokio::process::Command as TokioCommand;

        let marker = std::env::temp_dir().join(format!(
            "operon-job-object-marker-{}.txt",
            std::process::id()
        ));
        let _ = fs::remove_file(&marker);
        let marker_arg = marker.display().to_string().replace('\'', "''");
        let child_command = format!(
            "Start-Sleep -Seconds 3; Set-Content -LiteralPath '{}' -Value child",
            marker_arg
        );
        let parent_command = format!(
            "start \"\" /B powershell.exe -NoProfile -Command \"{}\" & timeout /T 30 /NOBREAK > NUL",
            child_command.replace('"', "\\\"")
        );

        let mut command = TokioCommand::new("cmd.exe");
        command
            .arg("/C")
            .arg(parent_command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().expect("spawn parent process");
        let child_group = ExecChildGroup::attach(&child);

        tokio::time::sleep(Duration::from_millis(500)).await;
        terminate_child(&mut child, &child_group).await;
        tokio::time::sleep(Duration::from_secs(4)).await;

        assert!(
            !marker.exists(),
            "descendant process survived Job Object termination and wrote {}",
            marker.display()
        );
    }
}
