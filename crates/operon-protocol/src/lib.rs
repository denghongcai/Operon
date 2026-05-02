pub const PROTOCOL_VERSION: &str = "v0.6.12";

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

impl From<operon_core::FsStat> for runtime::v1::FsStat {
    fn from(value: operon_core::FsStat) -> Self {
        Self {
            path: value.path,
            is_file: value.is_file,
            is_dir: value.is_dir,
            size: value.size,
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
        }
    }
}

impl From<operon_core::FsList> for runtime::v1::FsList {
    fn from(value: operon_core::FsList) -> Self {
        Self {
            path: value.path,
            entries: value.entries.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<runtime::v1::FsList> for operon_core::FsList {
    fn from(value: runtime::v1::FsList) -> Self {
        Self {
            path: value.path,
            entries: value.entries.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<operon_core::FsWrite> for runtime::v1::FsWrite {
    fn from(value: operon_core::FsWrite) -> Self {
        Self {
            path: value.path,
            bytes_written: value.bytes_written,
        }
    }
}

impl From<runtime::v1::FsWrite> for operon_core::FsWrite {
    fn from(value: runtime::v1::FsWrite) -> Self {
        Self {
            path: value.path,
            bytes_written: value.bytes_written,
        }
    }
}

impl From<operon_core::JobLog> for runtime::v1::JobLog {
    fn from(value: operon_core::JobLog) -> Self {
        Self {
            stream: value.stream,
            data: value.data,
            sequence: value.sequence,
        }
    }
}

impl From<runtime::v1::JobLog> for operon_core::JobLog {
    fn from(value: runtime::v1::JobLog) -> Self {
        Self {
            stream: value.stream,
            data: value.data,
            sequence: value.sequence,
        }
    }
}

impl From<operon_core::JobRecord> for runtime::v1::JobRecord {
    fn from(value: operon_core::JobRecord) -> Self {
        Self {
            id: value.id,
            node_id: value.node_id,
            command: value.command,
            cwd: value.cwd,
            status: grpc_job_status(&value.status) as i32,
            exit_code: value.exit_code,
            log_count: value.log_count,
            logs_truncated: value.logs_truncated,
        }
    }
}

impl TryFrom<runtime::v1::JobRecord> for operon_core::JobRecord {
    type Error = String;

    fn try_from(value: runtime::v1::JobRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            node_id: value.node_id,
            command: value.command,
            cwd: value.cwd,
            status: parse_grpc_job_status(value.status)?,
            exit_code: value.exit_code,
            log_count: value.log_count,
            logs_truncated: value.logs_truncated,
        })
    }
}

impl From<operon_core::JobLogList> for runtime::v1::JobLogList {
    fn from(value: operon_core::JobLogList) -> Self {
        Self {
            job_id: value.job_id,
            logs: value.logs.into_iter().map(Into::into).collect(),
            truncated: value.truncated,
            dropped_log_count: value.dropped_log_count,
        }
    }
}

impl From<runtime::v1::JobLogList> for operon_core::JobLogList {
    fn from(value: runtime::v1::JobLogList) -> Self {
        Self {
            job_id: value.job_id,
            logs: value.logs.into_iter().map(Into::into).collect(),
            truncated: value.truncated,
            dropped_log_count: value.dropped_log_count,
        }
    }
}

impl From<operon_core::JobEvent> for runtime::v1::JobEvent {
    fn from(value: operon_core::JobEvent) -> Self {
        Self {
            job_id: value.job_id,
            status: grpc_job_status(&value.status) as i32,
            exit_code: value.exit_code,
            log_count: value.log_count,
            logs_truncated: value.logs_truncated,
        }
    }
}

impl TryFrom<runtime::v1::JobEvent> for operon_core::JobEvent {
    type Error = String;

    fn try_from(value: runtime::v1::JobEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            job_id: value.job_id,
            status: parse_grpc_job_status(value.status)?,
            exit_code: value.exit_code,
            log_count: value.log_count,
            logs_truncated: value.logs_truncated,
        })
    }
}

impl From<operon_core::JobList> for runtime::v1::JobList {
    fn from(value: operon_core::JobList) -> Self {
        Self {
            jobs: value.jobs.into_iter().map(Into::into).collect(),
            next_page_token: value.next_page_token,
        }
    }
}

impl TryFrom<runtime::v1::JobList> for operon_core::JobList {
    type Error = String;

    fn try_from(value: runtime::v1::JobList) -> Result<Self, Self::Error> {
        Ok(Self {
            jobs: value
                .jobs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_page_token: value.next_page_token,
        })
    }
}

impl From<operon_core::JobStdin> for runtime::v1::JobStdin {
    fn from(value: operon_core::JobStdin) -> Self {
        Self {
            job_id: value.job_id,
            bytes_written: value.bytes_written,
        }
    }
}

impl From<runtime::v1::JobStdin> for operon_core::JobStdin {
    fn from(value: runtime::v1::JobStdin) -> Self {
        Self {
            job_id: value.job_id,
            bytes_written: value.bytes_written,
        }
    }
}

impl From<operon_core::JobStdinClose> for runtime::v1::JobStdinClose {
    fn from(value: operon_core::JobStdinClose) -> Self {
        Self {
            job_id: value.job_id,
            closed: value.closed,
        }
    }
}

impl From<runtime::v1::JobStdinClose> for operon_core::JobStdinClose {
    fn from(value: runtime::v1::JobStdinClose) -> Self {
        Self {
            job_id: value.job_id,
            closed: value.closed,
        }
    }
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
        })
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
            timestamp_ms: value.timestamp_ms as u64,
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
            timestamp_ms: value.timestamp_ms as u128,
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

pub fn format_job_status(status: &operon_core::JobStatus) -> &'static str {
    match status {
        operon_core::JobStatus::Running => "running",
        operon_core::JobStatus::Succeeded => "succeeded",
        operon_core::JobStatus::Failed => "failed",
        operon_core::JobStatus::Cancelled => "cancelled",
        operon_core::JobStatus::TimedOut => "timed-out",
    }
}

pub fn parse_job_status(value: &str) -> Result<operon_core::JobStatus, String> {
    match value {
        "running" => Ok(operon_core::JobStatus::Running),
        "succeeded" => Ok(operon_core::JobStatus::Succeeded),
        "failed" => Ok(operon_core::JobStatus::Failed),
        "cancelled" => Ok(operon_core::JobStatus::Cancelled),
        "timed-out" => Ok(operon_core::JobStatus::TimedOut),
        _ => Err(format!("unknown job status `{value}`")),
    }
}

fn grpc_capability_kind(kind: &operon_core::CapabilityKind) -> runtime::v1::CapabilityKind {
    match kind {
        operon_core::CapabilityKind::Fs => runtime::v1::CapabilityKind::Fs,
        operon_core::CapabilityKind::Process => runtime::v1::CapabilityKind::Process,
        operon_core::CapabilityKind::Job => runtime::v1::CapabilityKind::Job,
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
        runtime::v1::CapabilityKind::Job => Ok(operon_core::CapabilityKind::Job),
        runtime::v1::CapabilityKind::DeviceInfo => Ok(operon_core::CapabilityKind::DeviceInfo),
        runtime::v1::CapabilityKind::Service => Ok(operon_core::CapabilityKind::Service),
        runtime::v1::CapabilityKind::Unspecified => {
            Err("capability kind is unspecified".to_string())
        }
    }
}

fn grpc_job_status(status: &operon_core::JobStatus) -> runtime::v1::JobStatus {
    match status {
        operon_core::JobStatus::Running => runtime::v1::JobStatus::Running,
        operon_core::JobStatus::Succeeded => runtime::v1::JobStatus::Succeeded,
        operon_core::JobStatus::Failed => runtime::v1::JobStatus::Failed,
        operon_core::JobStatus::Cancelled => runtime::v1::JobStatus::Cancelled,
        operon_core::JobStatus::TimedOut => runtime::v1::JobStatus::TimedOut,
    }
}

fn parse_grpc_job_status(value: i32) -> Result<operon_core::JobStatus, String> {
    match runtime::v1::JobStatus::try_from(value)
        .map_err(|_| format!("unknown job status `{value}`"))?
    {
        runtime::v1::JobStatus::Running => Ok(operon_core::JobStatus::Running),
        runtime::v1::JobStatus::Succeeded => Ok(operon_core::JobStatus::Succeeded),
        runtime::v1::JobStatus::Failed => Ok(operon_core::JobStatus::Failed),
        runtime::v1::JobStatus::Cancelled => Ok(operon_core::JobStatus::Cancelled),
        runtime::v1::JobStatus::TimedOut => Ok(operon_core::JobStatus::TimedOut),
        runtime::v1::JobStatus::Unspecified => Err("job status is unspecified".to_string()),
    }
}

fn grpc_service_protocol(protocol: &operon_core::ServiceProtocol) -> runtime::v1::ServiceProtocol {
    match protocol {
        operon_core::ServiceProtocol::Tcp => runtime::v1::ServiceProtocol::Tcp,
    }
}

fn parse_grpc_service_protocol(value: i32) -> Result<operon_core::ServiceProtocol, String> {
    match runtime::v1::ServiceProtocol::try_from(value)
        .map_err(|_| format!("unknown service protocol `{value}`"))?
    {
        runtime::v1::ServiceProtocol::Tcp => Ok(operon_core::ServiceProtocol::Tcp),
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
        assert_eq!(PROTOCOL_VERSION, "v0.6.12");
    }

    #[test]
    fn list_conversions_preserve_page_tokens() {
        let capabilities = operon_core::CapabilityList {
            capabilities: Vec::new(),
            next_page_token: "cap-next".to_string(),
        };
        let grpc: runtime::v1::CapabilityList = capabilities.into();
        assert_eq!(grpc.next_page_token, "cap-next");
        let core = operon_core::CapabilityList::try_from(grpc).expect("capability list");
        assert_eq!(core.next_page_token, "cap-next");

        let jobs = operon_core::JobList {
            jobs: Vec::new(),
            next_page_token: "job-next".to_string(),
        };
        let grpc: runtime::v1::JobList = jobs.into();
        assert_eq!(grpc.next_page_token, "job-next");
        let core = operon_core::JobList::try_from(grpc).expect("job list");
        assert_eq!(core.next_page_token, "job-next");

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
}
