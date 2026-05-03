pub const PROTOCOL_VERSION: &str = "v0.11.0";

pub mod runtime {
    pub mod v1 {
        tonic::include_proto!("operon.runtime.v1");
    }
}

impl From<operon_core::HealthStatus> for runtime::v1::HealthStatus {
    fn from(value: operon_core::HealthStatus) -> Self {
        Self {
            ok: value.ok,
            node_id: value.node_id,
            version: value.version,
        }
    }
}

impl From<runtime::v1::HealthStatus> for operon_core::HealthStatus {
    fn from(value: runtime::v1::HealthStatus) -> Self {
        Self {
            ok: value.ok,
            node_id: value.node_id,
            version: value.version,
        }
    }
}

impl From<operon_core::NodeInfo> for runtime::v1::NodeInfo {
    fn from(value: operon_core::NodeInfo) -> Self {
        Self {
            id: value.id,
            hostname: value.hostname,
            os: value.os,
            arch: value.arch,
        }
    }
}

impl From<runtime::v1::NodeInfo> for operon_core::NodeInfo {
    fn from(value: runtime::v1::NodeInfo) -> Self {
        Self {
            id: value.id,
            hostname: value.hostname,
            os: value.os,
            arch: value.arch,
        }
    }
}

impl From<operon_core::Capability> for runtime::v1::Capability {
    fn from(value: operon_core::Capability) -> Self {
        Self {
            id: value.id,
            kind: grpc_capability_kind(&value.kind) as i32,
            node_id: value.node_id,
            name: value.name,
            permissions: value.permissions,
            description: value.description,
        }
    }
}

impl TryFrom<runtime::v1::Capability> for operon_core::Capability {
    type Error = String;

    fn try_from(value: runtime::v1::Capability) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            kind: parse_grpc_capability_kind(value.kind)?,
            node_id: value.node_id,
            name: value.name,
            permissions: value.permissions,
            description: value.description,
        })
    }
}

impl From<operon_core::CapabilityList> for runtime::v1::CapabilityList {
    fn from(value: operon_core::CapabilityList) -> Self {
        Self {
            capabilities: value.capabilities.into_iter().map(Into::into).collect(),
            next_page_token: value.next_page_token,
        }
    }
}

impl TryFrom<runtime::v1::CapabilityList> for operon_core::CapabilityList {
    type Error = String;

    fn try_from(value: runtime::v1::CapabilityList) -> Result<Self, Self::Error> {
        Ok(Self {
            capabilities: value
                .capabilities
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_page_token: value.next_page_token,
        })
    }
}

impl From<operon_core::CapabilityDiagnosticRequest> for runtime::v1::CapabilityDiagnosticRequest {
    fn from(value: operon_core::CapabilityDiagnosticRequest) -> Self {
        Self {
            capability_id: value.capability_id,
            action: value.action,
            resource: value.resource,
            timeout_secs: value.timeout_secs,
        }
    }
}

impl From<runtime::v1::CapabilityDiagnosticRequest> for operon_core::CapabilityDiagnosticRequest {
    fn from(value: runtime::v1::CapabilityDiagnosticRequest) -> Self {
        Self {
            capability_id: value.capability_id,
            action: value.action,
            resource: value.resource,
            timeout_secs: value.timeout_secs,
        }
    }
}

impl From<operon_core::PolicyDecision> for runtime::v1::PolicyDecision {
    fn from(value: operon_core::PolicyDecision) -> Self {
        Self {
            subject: value.subject,
            capability_id: value.capability_id,
            action: value.action,
            resource: value.resource,
            allowed: value.allowed,
            reason_code: value.reason_code.as_str().to_string(),
            message: value.message,
        }
    }
}

impl TryFrom<runtime::v1::PolicyDecision> for operon_core::PolicyDecision {
    type Error = String;

    fn try_from(value: runtime::v1::PolicyDecision) -> Result<Self, Self::Error> {
        let reason_code =
            operon_core::PolicyReasonCode::from_code(&value.reason_code).ok_or_else(|| {
                format!(
                    "unknown policy decision reason code `{}`",
                    value.reason_code
                )
            })?;
        Ok(Self {
            subject: value.subject,
            capability_id: value.capability_id,
            action: value.action,
            resource: value.resource,
            allowed: value.allowed,
            reason_code,
            message: value.message,
        })
    }
}

impl From<operon_core::FsStat> for runtime::v1::FsStat {
    fn from(value: operon_core::FsStat) -> Self {
        Self {
            path: value.path,
            is_file: value.is_file,
            is_dir: value.is_dir,
            size: value.size,
            version: value.version,
        }
    }
}

impl From<runtime::v1::FsStat> for operon_core::FsStat {
    fn from(value: runtime::v1::FsStat) -> Self {
        Self {
            path: value.path,
            is_file: value.is_file,
            is_dir: value.is_dir,
            size: value.size,
            version: value.version,
        }
    }
}

impl From<operon_core::FsEntry> for runtime::v1::FsEntry {
    fn from(value: operon_core::FsEntry) -> Self {
        Self {
            name: value.name,
            path: value.path,
            is_file: value.is_file,
            is_dir: value.is_dir,
            size: value.size,
            version: value.version,
        }
    }
}

impl From<runtime::v1::FsEntry> for operon_core::FsEntry {
    fn from(value: runtime::v1::FsEntry) -> Self {
        Self {
            name: value.name,
            path: value.path,
            is_file: value.is_file,
            is_dir: value.is_dir,
            size: value.size,
            version: value.version,
        }
    }
}

impl From<operon_core::FsList> for runtime::v1::FsList {
    fn from(value: operon_core::FsList) -> Self {
        Self {
            path: value.path,
            entries: value.entries.into_iter().map(Into::into).collect(),
            next_page_token: value.next_page_token,
        }
    }
}

impl From<runtime::v1::FsList> for operon_core::FsList {
    fn from(value: runtime::v1::FsList) -> Self {
        Self {
            path: value.path,
            entries: value.entries.into_iter().map(Into::into).collect(),
            next_page_token: value.next_page_token,
        }
    }
}

impl From<operon_core::FsReadRangeRequest> for runtime::v1::FsReadRangeRequest {
    fn from(value: operon_core::FsReadRangeRequest) -> Self {
        Self {
            path: value.path,
            offset: value.offset,
            size: value.size,
        }
    }
}

impl From<runtime::v1::FsReadRangeRequest> for operon_core::FsReadRangeRequest {
    fn from(value: runtime::v1::FsReadRangeRequest) -> Self {
        Self {
            path: value.path,
            offset: value.offset,
            size: value.size,
        }
    }
}

impl From<operon_core::FsWrite> for runtime::v1::FsWrite {
    fn from(value: operon_core::FsWrite) -> Self {
        Self {
            path: value.path,
            bytes_written: value.bytes_written,
            version: value.version,
        }
    }
}

impl From<runtime::v1::FsWrite> for operon_core::FsWrite {
    fn from(value: runtime::v1::FsWrite) -> Self {
        Self {
            path: value.path,
            bytes_written: value.bytes_written,
            version: value.version,
        }
    }
}

impl From<operon_core::FsPrecondition> for runtime::v1::FsPrecondition {
    fn from(value: operon_core::FsPrecondition) -> Self {
        Self {
            expected_version: value.expected_version,
            require_absent: value.require_absent,
        }
    }
}

impl From<runtime::v1::FsPrecondition> for operon_core::FsPrecondition {
    fn from(value: runtime::v1::FsPrecondition) -> Self {
        Self {
            expected_version: value.expected_version,
            require_absent: value.require_absent,
        }
    }
}

impl From<operon_core::ExecLog> for runtime::v1::ExecLog {
    fn from(value: operon_core::ExecLog) -> Self {
        Self {
            stream: value.stream,
            data: value.data,
            sequence: value.sequence,
        }
    }
}

impl From<runtime::v1::ExecLog> for operon_core::ExecLog {
    fn from(value: runtime::v1::ExecLog) -> Self {
        Self {
            stream: value.stream,
            data: value.data,
            sequence: value.sequence,
        }
    }
}

impl From<operon_core::ExecRunRequest> for runtime::v1::ExecRunRequest {
    fn from(value: operon_core::ExecRunRequest) -> Self {
        Self {
            command: value.command,
            cwd: value.cwd.unwrap_or_default(),
            timeout_secs: value.timeout_secs,
            secrets: value.secrets,
            argv: value.argv,
        }
    }
}

impl From<runtime::v1::ExecRunRequest> for operon_core::ExecRunRequest {
    fn from(value: runtime::v1::ExecRunRequest) -> Self {
        Self {
            command: value.command,
            argv: value.argv,
            cwd: (!value.cwd.is_empty()).then_some(value.cwd),
            timeout_secs: value.timeout_secs,
            secrets: value.secrets,
        }
    }
}

impl From<operon_core::ExecRecord> for runtime::v1::ExecRecord {
    fn from(value: operon_core::ExecRecord) -> Self {
        Self {
            id: value.id,
            node_id: value.node_id,
            command: value.command,
            cwd: value.cwd,
            status: grpc_exec_status(&value.status) as i32,
            exit_code: value.exit_code,
            log_count: value.log_count,
            logs_truncated: value.logs_truncated,
        }
    }
}

impl TryFrom<runtime::v1::ExecRecord> for operon_core::ExecRecord {
    type Error = String;

    fn try_from(value: runtime::v1::ExecRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            node_id: value.node_id,
            command: value.command,
            cwd: value.cwd,
            status: parse_grpc_exec_status(value.status)?,
            exit_code: value.exit_code,
            log_count: value.log_count,
            logs_truncated: value.logs_truncated,
        })
    }
}

impl From<operon_core::ExecLogList> for runtime::v1::ExecLogList {
    fn from(value: operon_core::ExecLogList) -> Self {
        Self {
            exec_id: value.exec_id,
            logs: value.logs.into_iter().map(Into::into).collect(),
            truncated: value.truncated,
            dropped_log_count: value.dropped_log_count,
        }
    }
}

impl From<runtime::v1::ExecLogList> for operon_core::ExecLogList {
    fn from(value: runtime::v1::ExecLogList) -> Self {
        Self {
            exec_id: value.exec_id,
            logs: value.logs.into_iter().map(Into::into).collect(),
            truncated: value.truncated,
            dropped_log_count: value.dropped_log_count,
        }
    }
}

impl From<operon_core::ExecEvent> for runtime::v1::ExecEvent {
    fn from(value: operon_core::ExecEvent) -> Self {
        Self {
            exec_id: value.exec_id,
            status: grpc_exec_status(&value.status) as i32,
            exit_code: value.exit_code,
            log_count: value.log_count,
            logs_truncated: value.logs_truncated,
        }
    }
}

impl TryFrom<runtime::v1::ExecEvent> for operon_core::ExecEvent {
    type Error = String;

    fn try_from(value: runtime::v1::ExecEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            exec_id: value.exec_id,
            status: parse_grpc_exec_status(value.status)?,
            exit_code: value.exit_code,
            log_count: value.log_count,
            logs_truncated: value.logs_truncated,
        })
    }
}

impl From<operon_core::ExecList> for runtime::v1::ExecList {
    fn from(value: operon_core::ExecList) -> Self {
        Self {
            execs: value.execs.into_iter().map(Into::into).collect(),
            next_page_token: value.next_page_token,
        }
    }
}

impl TryFrom<runtime::v1::ExecList> for operon_core::ExecList {
    type Error = String;

    fn try_from(value: runtime::v1::ExecList) -> Result<Self, Self::Error> {
        Ok(Self {
            execs: value
                .execs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_page_token: value.next_page_token,
        })
    }
}

impl From<operon_core::ExecStdin> for runtime::v1::ExecStdin {
    fn from(value: operon_core::ExecStdin) -> Self {
        Self {
            exec_id: value.exec_id,
            bytes_written: value.bytes_written,
        }
    }
}

impl From<runtime::v1::ExecStdin> for operon_core::ExecStdin {
    fn from(value: runtime::v1::ExecStdin) -> Self {
        Self {
            exec_id: value.exec_id,
            bytes_written: value.bytes_written,
        }
    }
}

impl From<operon_core::ExecStdinClose> for runtime::v1::ExecStdinClose {
    fn from(value: operon_core::ExecStdinClose) -> Self {
        Self {
            exec_id: value.exec_id,
            closed: value.closed,
        }
    }
}

impl From<runtime::v1::ExecStdinClose> for operon_core::ExecStdinClose {
    fn from(value: runtime::v1::ExecStdinClose) -> Self {
        Self {
            exec_id: value.exec_id,
            closed: value.closed,
        }
    }
}

impl From<operon_core::ExecSessionStart> for runtime::v1::ExecSessionStart {
    fn from(value: operon_core::ExecSessionStart) -> Self {
        Self {
            command: value.command,
            cwd: value.cwd.unwrap_or_default(),
            timeout_secs: value.timeout_secs,
            secrets: value.secrets,
            argv: value.argv,
            rows: value.rows as u32,
            cols: value.cols as u32,
        }
    }
}

impl TryFrom<runtime::v1::ExecSessionStart> for operon_core::ExecSessionStart {
    type Error = String;

    fn try_from(value: runtime::v1::ExecSessionStart) -> Result<Self, Self::Error> {
        Ok(Self {
            command: value.command,
            argv: value.argv,
            cwd: (!value.cwd.is_empty()).then_some(value.cwd),
            timeout_secs: value.timeout_secs,
            secrets: value.secrets,
            rows: session_dimension(value.rows, 24, "rows")?,
            cols: session_dimension(value.cols, 80, "cols")?,
        })
    }
}

impl From<operon_core::ExecSessionStarted> for runtime::v1::ExecSessionStarted {
    fn from(value: operon_core::ExecSessionStarted) -> Self {
        Self {
            exec_id: value.exec_id,
        }
    }
}

impl From<operon_core::ExecSessionOutput> for runtime::v1::ExecSessionOutput {
    fn from(value: operon_core::ExecSessionOutput) -> Self {
        Self {
            exec_id: value.exec_id,
            data: value.data,
        }
    }
}

impl From<operon_core::ExecSessionExit> for runtime::v1::ExecSessionExit {
    fn from(value: operon_core::ExecSessionExit) -> Self {
        Self {
            exec_id: value.exec_id,
            status: grpc_exec_status(&value.status) as i32,
            exit_code: value.exit_code,
        }
    }
}

impl From<operon_core::ExecSessionEvent> for runtime::v1::ExecSessionEvent {
    fn from(value: operon_core::ExecSessionEvent) -> Self {
        use runtime::v1::exec_session_event::Event;
        let event = match value {
            operon_core::ExecSessionEvent::Started(started) => Event::Started(started.into()),
            operon_core::ExecSessionEvent::Output(output) => Event::Output(output.into()),
            operon_core::ExecSessionEvent::Exit(exit) => Event::Exit(exit.into()),
        };
        Self { event: Some(event) }
    }
}

fn session_dimension(value: u32, default: u16, field: &str) -> Result<u16, String> {
    if value == 0 {
        return Ok(default);
    }
    u16::try_from(value).map_err(|_| format!("exec session {field} is out of range"))
}

impl From<operon_core::ServiceDefinition> for runtime::v1::ServiceDefinition {
    fn from(value: operon_core::ServiceDefinition) -> Self {
        Self {
            id: value.id,
            name: value.name,
            host: value.host,
            port: value.port as u32,
            protocol: grpc_service_protocol(&value.protocol) as i32,
            description: value.description,
            permissions: Some(value.permissions.into()),
        }
    }
}

impl TryFrom<runtime::v1::ServiceDefinition> for operon_core::ServiceDefinition {
    type Error = String;

    fn try_from(value: runtime::v1::ServiceDefinition) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            name: value.name,
            host: value.host,
            port: u16::try_from(value.port).map_err(|_| "service port out of range")?,
            protocol: parse_grpc_service_protocol(value.protocol)?,
            description: value.description,
            permissions: value.permissions.map(Into::into).unwrap_or_default(),
        })
    }
}

impl From<operon_core::ServicePermissions> for runtime::v1::ServicePermissions {
    fn from(value: operon_core::ServicePermissions) -> Self {
        Self {
            check: value.check,
            forward: value.forward,
        }
    }
}

impl From<runtime::v1::ServicePermissions> for operon_core::ServicePermissions {
    fn from(value: runtime::v1::ServicePermissions) -> Self {
        Self {
            check: value.check,
            forward: value.forward,
        }
    }
}

impl From<operon_core::ServiceList> for runtime::v1::ServiceList {
    fn from(value: operon_core::ServiceList) -> Self {
        Self {
            services: value.services.into_iter().map(Into::into).collect(),
            next_page_token: value.next_page_token,
        }
    }
}

impl TryFrom<runtime::v1::ServiceList> for operon_core::ServiceList {
    type Error = String;

    fn try_from(value: runtime::v1::ServiceList) -> Result<Self, Self::Error> {
        Ok(Self {
            services: value
                .services
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_page_token: value.next_page_token,
        })
    }
}

impl From<operon_core::ServiceCheck> for runtime::v1::ServiceCheck {
    fn from(value: operon_core::ServiceCheck) -> Self {
        Self {
            id: value.id,
            ok: value.ok,
            latency_ms: value.latency_ms as u64,
            reason: value.reason,
        }
    }
}

impl From<runtime::v1::ServiceCheck> for operon_core::ServiceCheck {
    fn from(value: runtime::v1::ServiceCheck) -> Self {
        Self {
            id: value.id,
            ok: value.ok,
            latency_ms: value.latency_ms as u128,
            reason: value.reason,
        }
    }
}

impl From<operon_core::AuditEvent> for runtime::v1::AuditEvent {
    fn from(value: operon_core::AuditEvent) -> Self {
        Self {
            subject: value.subject,
            timestamp_ms: value.timestamp_ms,
            node_id: value.node_id,
            capability: value.capability,
            action: value.action,
            resource: value.resource,
            allowed: value.allowed,
            reason: value.reason,
            run_id: value.run_id,
            step_id: value.step_id,
        }
    }
}

impl From<runtime::v1::AuditEvent> for operon_core::AuditEvent {
    fn from(value: runtime::v1::AuditEvent) -> Self {
        Self {
            subject: value.subject,
            timestamp_ms: value.timestamp_ms,
            node_id: value.node_id,
            capability: value.capability,
            action: value.action,
            resource: value.resource,
            allowed: value.allowed,
            reason: value.reason,
            run_id: value.run_id,
            step_id: value.step_id,
        }
    }
}

impl From<operon_core::AuditLog> for runtime::v1::AuditLog {
    fn from(value: operon_core::AuditLog) -> Self {
        Self {
            events: value.events.into_iter().map(Into::into).collect(),
            next_page_token: value.next_page_token,
        }
    }
}

impl From<runtime::v1::AuditLog> for operon_core::AuditLog {
    fn from(value: runtime::v1::AuditLog) -> Self {
        Self {
            events: value.events.into_iter().map(Into::into).collect(),
            next_page_token: value.next_page_token,
        }
    }
}

pub fn format_exec_status(status: &operon_core::ExecStatus) -> &'static str {
    match status {
        operon_core::ExecStatus::Running => "running",
        operon_core::ExecStatus::Succeeded => "succeeded",
        operon_core::ExecStatus::Failed => "failed",
        operon_core::ExecStatus::Cancelled => "cancelled",
        operon_core::ExecStatus::TimedOut => "timed-out",
    }
}

pub fn parse_exec_status(value: &str) -> Result<operon_core::ExecStatus, String> {
    match value {
        "running" => Ok(operon_core::ExecStatus::Running),
        "succeeded" => Ok(operon_core::ExecStatus::Succeeded),
        "failed" => Ok(operon_core::ExecStatus::Failed),
        "cancelled" => Ok(operon_core::ExecStatus::Cancelled),
        "timed-out" => Ok(operon_core::ExecStatus::TimedOut),
        _ => Err(format!("unknown exec status `{value}`")),
    }
}

fn grpc_capability_kind(kind: &operon_core::CapabilityKind) -> runtime::v1::CapabilityKind {
    match kind {
        operon_core::CapabilityKind::Fs => runtime::v1::CapabilityKind::Fs,
        operon_core::CapabilityKind::Process => runtime::v1::CapabilityKind::Process,
        operon_core::CapabilityKind::Exec => runtime::v1::CapabilityKind::Exec,
        operon_core::CapabilityKind::DeviceInfo => runtime::v1::CapabilityKind::DeviceInfo,
        operon_core::CapabilityKind::Service => runtime::v1::CapabilityKind::Service,
    }
}

fn parse_grpc_capability_kind(value: i32) -> Result<operon_core::CapabilityKind, String> {
    match runtime::v1::CapabilityKind::try_from(value)
        .map_err(|_| format!("unknown capability kind `{value}`"))?
    {
        runtime::v1::CapabilityKind::Fs => Ok(operon_core::CapabilityKind::Fs),
        runtime::v1::CapabilityKind::Process => Ok(operon_core::CapabilityKind::Process),
        runtime::v1::CapabilityKind::Exec => Ok(operon_core::CapabilityKind::Exec),
        runtime::v1::CapabilityKind::DeviceInfo => Ok(operon_core::CapabilityKind::DeviceInfo),
        runtime::v1::CapabilityKind::Service => Ok(operon_core::CapabilityKind::Service),
        runtime::v1::CapabilityKind::Unspecified => {
            Err("capability kind is unspecified".to_string())
        }
    }
}

fn grpc_exec_status(status: &operon_core::ExecStatus) -> runtime::v1::ExecStatus {
    match status {
        operon_core::ExecStatus::Running => runtime::v1::ExecStatus::Running,
        operon_core::ExecStatus::Succeeded => runtime::v1::ExecStatus::Succeeded,
        operon_core::ExecStatus::Failed => runtime::v1::ExecStatus::Failed,
        operon_core::ExecStatus::Cancelled => runtime::v1::ExecStatus::Cancelled,
        operon_core::ExecStatus::TimedOut => runtime::v1::ExecStatus::TimedOut,
    }
}

fn parse_grpc_exec_status(value: i32) -> Result<operon_core::ExecStatus, String> {
    match runtime::v1::ExecStatus::try_from(value)
        .map_err(|_| format!("unknown exec status `{value}`"))?
    {
        runtime::v1::ExecStatus::Running => Ok(operon_core::ExecStatus::Running),
        runtime::v1::ExecStatus::Succeeded => Ok(operon_core::ExecStatus::Succeeded),
        runtime::v1::ExecStatus::Failed => Ok(operon_core::ExecStatus::Failed),
        runtime::v1::ExecStatus::Cancelled => Ok(operon_core::ExecStatus::Cancelled),
        runtime::v1::ExecStatus::TimedOut => Ok(operon_core::ExecStatus::TimedOut),
        runtime::v1::ExecStatus::Unspecified => Err("exec status is unspecified".to_string()),
    }
}

fn grpc_service_protocol(protocol: &operon_core::ServiceProtocol) -> runtime::v1::ServiceProtocol {
    match protocol {
        operon_core::ServiceProtocol::Tcp => runtime::v1::ServiceProtocol::Tcp,
        operon_core::ServiceProtocol::Udp => runtime::v1::ServiceProtocol::Udp,
    }
}

fn parse_grpc_service_protocol(value: i32) -> Result<operon_core::ServiceProtocol, String> {
    match runtime::v1::ServiceProtocol::try_from(value)
        .map_err(|_| format!("unknown service protocol `{value}`"))?
    {
        runtime::v1::ServiceProtocol::Tcp => Ok(operon_core::ServiceProtocol::Tcp),
        runtime::v1::ServiceProtocol::Udp => Ok(operon_core::ServiceProtocol::Udp),
        runtime::v1::ServiceProtocol::Unspecified => {
            Err("service protocol is unspecified".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_matches_grpc_release_line() {
        assert_eq!(PROTOCOL_VERSION, "v0.11.0");
    }

    #[test]
    fn list_conversions_preserve_page_tokens() {
        let fs_list = operon_core::FsList {
            path: "/".to_string(),
            entries: Vec::new(),
            next_page_token: "fs-next".to_string(),
        };
        let grpc: runtime::v1::FsList = fs_list.into();
        assert_eq!(grpc.next_page_token, "fs-next");
        let core = operon_core::FsList::from(grpc);
        assert_eq!(core.next_page_token, "fs-next");

        let capabilities = operon_core::CapabilityList {
            capabilities: Vec::new(),
            next_page_token: "cap-next".to_string(),
        };
        let grpc: runtime::v1::CapabilityList = capabilities.into();
        assert_eq!(grpc.next_page_token, "cap-next");
        let core = operon_core::CapabilityList::try_from(grpc).expect("capability list");
        assert_eq!(core.next_page_token, "cap-next");

        let execs = operon_core::ExecList {
            execs: Vec::new(),
            next_page_token: "exec-next".to_string(),
        };
        let grpc: runtime::v1::ExecList = execs.into();
        assert_eq!(grpc.next_page_token, "exec-next");
        let core = operon_core::ExecList::try_from(grpc).expect("exec list");
        assert_eq!(core.next_page_token, "exec-next");

        let services = operon_core::ServiceList {
            services: Vec::new(),
            next_page_token: "service-next".to_string(),
        };
        let grpc: runtime::v1::ServiceList = services.into();
        assert_eq!(grpc.next_page_token, "service-next");
        let core = operon_core::ServiceList::try_from(grpc).expect("service list");
        assert_eq!(core.next_page_token, "service-next");

        let audit = operon_core::AuditLog {
            events: Vec::new(),
            next_page_token: "audit-next".to_string(),
        };
        let grpc: runtime::v1::AuditLog = audit.into();
        assert_eq!(grpc.next_page_token, "audit-next");
        let core = operon_core::AuditLog::from(grpc);
        assert_eq!(core.next_page_token, "audit-next");
    }

    #[test]
    fn fs_version_and_precondition_round_trip_through_grpc_shape() {
        let stat = operon_core::FsStat {
            path: "/file.txt".to_string(),
            is_file: true,
            is_dir: false,
            size: 10,
            version: "v1:file:10:123".to_string(),
        };
        let grpc: runtime::v1::FsStat = stat.clone().into();
        assert_eq!(grpc.version, stat.version);
        let core = operon_core::FsStat::from(grpc);
        assert_eq!(core.version, stat.version);

        let precondition = operon_core::FsPrecondition {
            expected_version: Some("v1:file:10:123".to_string()),
            require_absent: false,
        };
        let grpc: runtime::v1::FsPrecondition = precondition.clone().into();
        assert_eq!(grpc.expected_version, precondition.expected_version);
        let core = operon_core::FsPrecondition::from(grpc);
        assert_eq!(core, precondition);
    }

    #[test]
    fn policy_decision_round_trips_through_grpc_shape() {
        let decision = operon_core::PolicyDecision::denied(
            "local-cli",
            "exec:default",
            "run",
            "/tmp",
            operon_core::PolicyReasonCode::ExecCwdDenied,
            "exec cwd denied by policy",
        );

        let grpc: runtime::v1::PolicyDecision = decision.clone().into();
        assert_eq!(grpc.reason_code, "exec-cwd-denied");
        let core = operon_core::PolicyDecision::try_from(grpc).expect("policy decision");

        assert_eq!(core, decision);
    }

    #[test]
    fn fs_read_range_request_round_trips() {
        let request = operon_core::FsReadRangeRequest {
            path: "/large.bin".to_string(),
            offset: 4096,
            size: 8192,
        };

        let grpc: runtime::v1::FsReadRangeRequest = request.clone().into();
        assert_eq!(grpc.path, request.path);
        assert_eq!(grpc.offset, request.offset);
        assert_eq!(grpc.size, request.size);

        let core = operon_core::FsReadRangeRequest::from(grpc);
        assert_eq!(core.path, request.path);
        assert_eq!(core.offset, request.offset);
        assert_eq!(core.size, request.size);
    }

    #[test]
    fn audit_event_timestamp_round_trips_without_casting() {
        let event = operon_core::AuditEvent {
            subject: "subject".to_string(),
            timestamp_ms: u64::MAX - 1,
            node_id: "node-a".to_string(),
            capability: "fs:workspace".to_string(),
            action: "read".to_string(),
            resource: "/file.txt".to_string(),
            allowed: true,
            reason: "allowed".to_string(),
            run_id: Some("run-1".to_string()),
            step_id: Some("step-1".to_string()),
        };

        let grpc: runtime::v1::AuditEvent = event.clone().into();
        assert_eq!(grpc.timestamp_ms, event.timestamp_ms);
        let core = operon_core::AuditEvent::from(grpc);
        assert_eq!(core.timestamp_ms, event.timestamp_ms);
    }

    #[test]
    fn exec_run_request_preserves_argv_execution_fields() {
        let request = operon_core::ExecRunRequest {
            command: String::new(),
            argv: vec!["printf".to_string(), "hello world".to_string()],
            cwd: Some("/work".to_string()),
            timeout_secs: Some(10),
            secrets: vec!["TOKEN".to_string()],
        };

        let grpc: runtime::v1::ExecRunRequest = request.clone().into();
        let core: operon_core::ExecRunRequest = grpc.into();

        assert_eq!(core.command, "");
        assert_eq!(core.argv, request.argv);
        assert_eq!(core.cwd, Some("/work".to_string()));
        assert_eq!(core.timeout_secs, Some(10));
        assert_eq!(core.secrets, vec!["TOKEN".to_string()]);
    }

    #[test]
    fn exec_session_start_round_trips_through_grpc_shape() {
        let request = operon_core::ExecSessionStart {
            command: String::new(),
            argv: vec!["bash".to_string(), "-li".to_string()],
            cwd: Some("/work".to_string()),
            timeout_secs: Some(120),
            secrets: vec!["TOKEN".to_string()],
            rows: 33,
            cols: 120,
        };

        let grpc: runtime::v1::ExecSessionStart = request.clone().into();
        let core = operon_core::ExecSessionStart::try_from(grpc).expect("session start");

        assert_eq!(core.argv, request.argv);
        assert_eq!(core.cwd, request.cwd);
        assert_eq!(core.timeout_secs, request.timeout_secs);
        assert_eq!(core.secrets, request.secrets);
        assert_eq!(core.rows, 33);
        assert_eq!(core.cols, 120);
    }

    #[test]
    fn service_definition_permissions_round_trip() {
        let service = operon_core::ServiceDefinition {
            id: "web".to_string(),
            name: "web".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            protocol: operon_core::ServiceProtocol::Tcp,
            description: "test service".to_string(),
            permissions: operon_core::ServicePermissions {
                check: true,
                forward: false,
            },
        };

        let grpc: runtime::v1::ServiceDefinition = service.clone().into();
        assert_eq!(grpc.permissions.as_ref().expect("permissions").check, true);
        assert_eq!(
            grpc.permissions.as_ref().expect("permissions").forward,
            false
        );
        let core = operon_core::ServiceDefinition::try_from(grpc).expect("service definition");
        assert!(core.permissions.check);
        assert!(!core.permissions.forward);
    }
}
