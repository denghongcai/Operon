pub const PROTOCOL_VERSION: &str = "v0.5";

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
            kind: format_capability_kind(&value.kind).to_string(),
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
            kind: parse_capability_kind(&value.kind)?,
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
        }
    }
}

impl From<runtime::v1::JobLog> for operon_core::JobLog {
    fn from(value: runtime::v1::JobLog) -> Self {
        Self {
            stream: value.stream,
            data: value.data,
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
            status: format_job_status(&value.status).to_string(),
            logs: value.logs.into_iter().map(Into::into).collect(),
            exit_code: value.exit_code.unwrap_or_default(),
            has_exit_code: value.exit_code.is_some(),
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
            status: parse_job_status(&value.status)?,
            logs: value.logs.into_iter().map(Into::into).collect(),
            exit_code: value.has_exit_code.then_some(value.exit_code),
        })
    }
}

impl From<operon_core::JobList> for runtime::v1::JobList {
    fn from(value: operon_core::JobList) -> Self {
        Self {
            jobs: value.jobs.into_iter().map(Into::into).collect(),
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
            protocol: format_service_protocol(&value.protocol).to_string(),
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
            protocol: parse_service_protocol(&value.protocol)?,
            description: value.description,
        })
    }
}

impl From<operon_core::ServiceList> for runtime::v1::ServiceList {
    fn from(value: operon_core::ServiceList) -> Self {
        Self {
            services: value.services.into_iter().map(Into::into).collect(),
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
        })
    }
}

impl From<operon_core::ServiceCheck> for runtime::v1::ServiceCheck {
    fn from(value: operon_core::ServiceCheck) -> Self {
        Self {
            id: value.id,
            ok: value.ok,
            latency_ms: value.latency_ms as u64,
            reason: value.reason.clone().unwrap_or_default(),
            has_reason: value.reason.is_some(),
        }
    }
}

impl From<runtime::v1::ServiceCheck> for operon_core::ServiceCheck {
    fn from(value: runtime::v1::ServiceCheck) -> Self {
        Self {
            id: value.id,
            ok: value.ok,
            latency_ms: value.latency_ms as u128,
            reason: value.has_reason.then_some(value.reason),
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
            run_id: value.run_id.clone().unwrap_or_default(),
            step_id: value.step_id.clone().unwrap_or_default(),
            has_run_id: value.run_id.is_some(),
            has_step_id: value.step_id.is_some(),
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
            run_id: value.has_run_id.then_some(value.run_id),
            step_id: value.has_step_id.then_some(value.step_id),
        }
    }
}

impl From<operon_core::AuditLog> for runtime::v1::AuditLog {
    fn from(value: operon_core::AuditLog) -> Self {
        Self {
            events: value.events.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<runtime::v1::AuditLog> for operon_core::AuditLog {
    fn from(value: runtime::v1::AuditLog) -> Self {
        Self {
            events: value.events.into_iter().map(Into::into).collect(),
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

fn format_capability_kind(kind: &operon_core::CapabilityKind) -> &'static str {
    match kind {
        operon_core::CapabilityKind::Fs => "fs",
        operon_core::CapabilityKind::Process => "process",
        operon_core::CapabilityKind::Job => "job",
        operon_core::CapabilityKind::DeviceInfo => "device-info",
        operon_core::CapabilityKind::Service => "service",
    }
}

fn parse_capability_kind(value: &str) -> Result<operon_core::CapabilityKind, String> {
    match value {
        "fs" => Ok(operon_core::CapabilityKind::Fs),
        "process" => Ok(operon_core::CapabilityKind::Process),
        "job" => Ok(operon_core::CapabilityKind::Job),
        "device-info" => Ok(operon_core::CapabilityKind::DeviceInfo),
        "service" => Ok(operon_core::CapabilityKind::Service),
        _ => Err(format!("unknown capability kind `{value}`")),
    }
}

fn format_service_protocol(protocol: &operon_core::ServiceProtocol) -> &'static str {
    match protocol {
        operon_core::ServiceProtocol::Tcp => "tcp",
    }
}

fn parse_service_protocol(value: &str) -> Result<operon_core::ServiceProtocol, String> {
    match value {
        "tcp" => Ok(operon_core::ServiceProtocol::Tcp),
        _ => Err(format!("unknown service protocol `{value}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_matches_grpc_release_line() {
        assert_eq!(PROTOCOL_VERSION, "v0.5");
    }
}
