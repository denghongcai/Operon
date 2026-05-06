import {
  CapabilityKind as GrpcCapabilityKind,
  ExecStatus as GrpcExecStatus,
  ServiceProtocol as GrpcServiceProtocol,
  type AuditLog as GrpcAuditLog,
  type Capability as GrpcCapability,
  type PolicyDecision as GrpcPolicyDecision,
  type FsList as GrpcFsList,
  type FsStat as GrpcFsStat,
  type FsWrite as GrpcFsWrite,
  type ExecEvent as GrpcExecEvent,
  type ExecList as GrpcExecList,
  type ExecLog as GrpcExecLog,
  type ExecLogList as GrpcExecLogList,
  type ExecLogStreamEvent as GrpcExecLogStreamEvent,
  type ExecRecord as GrpcExecRecord,
  type ExecSessionEvent as GrpcExecSessionEvent,
  type ExecStdin as GrpcExecStdin,
  type ExecStdinClose as GrpcExecStdinClose,
  type ServiceCheck as GrpcServiceCheck,
  type ServiceDatagramTunnelResponse as GrpcServiceDatagramTunnelResponse,
  type ServiceList as GrpcServiceList,
  type ServiceTunnelResponse as GrpcServiceTunnelResponse,
} from "./generated/operon/runtime";
import type {
  AuditEvent,
  Capability,
  ExecEvent,
  ExecLog,
  ExecLogList,
  ExecLogStreamEvent,
  ExecRecord,
  ExecSessionEvent,
  ExecStdinCloseResult,
  ExecStdinResult,
  PolicyDecision,
  ServiceCheck,
  ServiceDatagramTunnelEvent,
  ServiceDefinition,
  ServiceList,
} from "./types";

export function serviceTunnelReadableStream(
  iterator: AsyncIterator<GrpcServiceTunnelResponse>,
): ReadableStream<Uint8Array> {
  return new ReadableStream<Uint8Array>({
    async pull(controller) {
      while (true) {
        const next = await iterator.next();
        if (next.done) {
          controller.close();
          return;
        }
        if (next.value.data) {
          controller.enqueue(next.value.data.data);
          return;
        }
        if (next.value.close) {
          controller.close();
          return;
        }
      }
    },
    async cancel() {
      if (iterator.return) {
        await iterator.return();
      }
    },
  });
}

export async function* mapGrpcServiceDatagramTunnelEvents(
  events: AsyncIterable<GrpcServiceDatagramTunnelResponse>,
): AsyncIterable<ServiceDatagramTunnelEvent> {
  for await (const event of events) {
    if (event.opened) {
      yield {
        type: "opened",
        service_id: event.opened.serviceId,
        host: event.opened.host,
        port: event.opened.port,
      };
    } else if (event.datagram) {
      yield {
        type: "datagram",
        peer_id: event.datagram.peerId,
        data: event.datagram.data,
      };
    } else if (event.close) {
      yield {
        type: "close",
        peer_id: event.close.peerId,
        reason: event.close.reason,
      };
    }
  }
}

export function fromGrpcFsStat(stat: GrpcFsStat) {
  return {
    path: stat.path,
    is_file: stat.isFile,
    is_dir: stat.isDir,
    size: Number(stat.size),
    version: stat.version,
  };
}

export function fromGrpcFsList(list: GrpcFsList) {
  return {
    path: list.path,
    entries: list.entries.map((entry) => ({
      name: entry.name,
      path: entry.path,
      is_file: entry.isFile,
      is_dir: entry.isDir,
      size: Number(entry.size),
      version: entry.version,
    })),
    next_page_token: list.nextPageToken,
  };
}

export function fromGrpcCapability(capability: GrpcCapability): Capability {
  return {
    id: capability.id,
    kind: fromGrpcCapabilityKind(capability.kind),
    node_id: capability.nodeId,
    name: capability.name,
    permissions: capability.permissions,
    description: capability.description,
  };
}

function fromGrpcCapabilityKind(kind: GrpcCapabilityKind): Capability["kind"] {
  switch (kind) {
    case GrpcCapabilityKind.CAPABILITY_KIND_FS:
      return "fs";
    case GrpcCapabilityKind.CAPABILITY_KIND_PROCESS:
      return "process";
    case GrpcCapabilityKind.CAPABILITY_KIND_EXEC:
      return "exec";
    case GrpcCapabilityKind.CAPABILITY_KIND_DEVICE_INFO:
      return "device-info";
    case GrpcCapabilityKind.CAPABILITY_KIND_SERVICE:
      return "service";
    case GrpcCapabilityKind.CAPABILITY_KIND_UNSPECIFIED:
      return "unspecified";
    default:
      return "unrecognized";
  }
}

export function fromGrpcPolicyDecision(decision: GrpcPolicyDecision): PolicyDecision {
  return {
    subject: decision.subject,
    capability_id: decision.capabilityId,
    action: decision.action,
    resource: decision.resource,
    allowed: decision.allowed,
    reason_code: decision.reasonCode,
    message: decision.message,
  };
}

export function fromGrpcFsWrite(write: GrpcFsWrite) {
  return {
    path: write.path,
    bytes_written: Number(write.bytesWritten),
    version: write.version,
  };
}

export function fromGrpcExecRecord(record: GrpcExecRecord): ExecRecord {
  return {
    id: record.id,
    node_id: record.nodeId,
    command: record.command,
    cwd: record.cwd,
    status: fromGrpcExecStatus(record.status),
    exit_code: record.exitCode ?? null,
    log_count: Number(record.logCount),
    logs_truncated: record.logsTruncated,
  };
}

function fromGrpcExecLog(log: GrpcExecLog): ExecLog {
  return {
    stream: log.stream,
    data: log.data,
    sequence: Number(log.sequence),
  };
}

export function fromGrpcExecLogList(list: GrpcExecLogList): ExecLogList {
  return {
    exec_id: list.execId,
    logs: list.logs.map(fromGrpcExecLog),
    truncated: list.truncated,
    dropped_log_count: Number(list.droppedLogCount),
  };
}

function fromGrpcExecLogStreamEvent(event: GrpcExecLogStreamEvent): ExecLogStreamEvent | undefined {
  if (event.snapshot) {
    return {
      type: "snapshot",
      snapshot: {
        exec_id: event.snapshot.execId,
        logs: event.snapshot.logs.map(fromGrpcExecLog),
        truncated: event.snapshot.truncated,
        dropped_log_count: Number(event.snapshot.droppedLogCount),
        next_sequence: Number(event.snapshot.nextSequence),
      },
    };
  }
  if (event.entry?.log) {
    return {
      type: "entry",
      exec_id: event.entry.execId,
      log: fromGrpcExecLog(event.entry.log),
    };
  }
  if (event.complete) {
    return {
      type: "complete",
      exec_id: event.complete.execId,
      status: fromGrpcExecStatus(event.complete.status),
      exit_code: event.complete.exitCode ?? null,
      log_count: Number(event.complete.logCount),
      logs_truncated: event.complete.logsTruncated,
      truncated: event.complete.truncated,
      dropped_log_count: Number(event.complete.droppedLogCount),
    };
  }
  return undefined;
}

export function fromGrpcExecEvent(event: GrpcExecEvent): ExecEvent {
  return {
    exec_id: event.execId,
    status: fromGrpcExecStatus(event.status),
    exit_code: event.exitCode ?? null,
    log_count: Number(event.logCount),
    logs_truncated: event.logsTruncated,
  };
}

export function fromGrpcAuditEvent(event: GrpcAuditLog["events"][number]): AuditEvent {
  return {
    subject: event.subject,
    timestamp_ms: Number(event.timestampMs),
    node_id: event.nodeId,
    capability: event.capability,
    action: event.action,
    resource: event.resource,
    allowed: event.allowed,
    reason: event.reason,
    run_id: event.runId ?? null,
    step_id: event.stepId ?? null,
  };
}

export async function* mapGrpcExecEvents(events: AsyncIterable<GrpcExecEvent>): AsyncIterable<ExecEvent> {
  for await (const event of events) {
    yield fromGrpcExecEvent(event);
  }
}

export async function* mapGrpcExecLogStreamEvents(
  events: AsyncIterable<GrpcExecLogStreamEvent>,
): AsyncIterable<ExecLogStreamEvent> {
  for await (const event of events) {
    const mapped = fromGrpcExecLogStreamEvent(event);
    if (mapped) {
      yield mapped;
    }
  }
}

export async function* mapGrpcExecSessionEvents(
  events: AsyncIterable<GrpcExecSessionEvent>,
): AsyncIterable<ExecSessionEvent> {
  for await (const event of events) {
    if (event.started) {
      yield { type: "started", exec_id: event.started.execId };
    } else if (event.output) {
      yield { type: "output", exec_id: event.output.execId, data: event.output.data };
    } else if (event.exit) {
      yield {
        type: "exit",
        exec_id: event.exit.execId,
        status: fromGrpcExecStatus(event.exit.status),
        exit_code: event.exit.exitCode,
      };
    }
  }
}

export async function* streamEventLogs(events: AsyncIterable<ExecLogStreamEvent>): AsyncIterable<Uint8Array> {
  let nextSequence = 0;
  for await (const event of events) {
    if (event.type === "snapshot") {
      for (const log of event.snapshot.logs) {
        if (log.sequence >= nextSequence) {
          nextSequence = log.sequence + 1;
          yield log.data;
        }
      }
      nextSequence = Math.max(nextSequence, event.snapshot.next_sequence);
    } else if (event.type === "entry" && event.log.sequence >= nextSequence) {
      nextSequence = event.log.sequence + 1;
      yield event.log.data;
    }
  }
}

export function fromGrpcExecList(list: GrpcExecList) {
  return {
    execs: list.execs.map(fromGrpcExecRecord),
  };
}

export function fromGrpcExecStdin(result: GrpcExecStdin): ExecStdinResult {
  return {
    exec_id: result.execId,
    bytes_written: Number(result.bytesWritten),
  };
}

export function fromGrpcExecStdinClose(result: GrpcExecStdinClose): ExecStdinCloseResult {
  return {
    exec_id: result.execId,
    closed: result.closed,
  };
}

export function fromGrpcServiceList(list: GrpcServiceList): ServiceList {
  return {
    services: list.services.map(fromGrpcServiceDefinition),
  };
}

export function fromGrpcServiceDefinition(service: GrpcServiceList["services"][number]): ServiceDefinition {
  return {
    id: service.id,
    name: service.name,
    host: service.host,
    port: service.port,
    protocol: fromGrpcServiceProtocol(service.protocol),
    description: service.description,
    permissions: service.permissions ?? { check: true, forward: true },
  };
}

export function fromGrpcServiceCheck(check: GrpcServiceCheck): ServiceCheck {
  return {
    id: check.id,
    ok: check.ok,
    latency_ms: Number(check.latencyMs),
    reason: check.reason ?? null,
  };
}

function fromGrpcExecStatus(status: GrpcExecStatus): ExecRecord["status"] {
  switch (status) {
    case GrpcExecStatus.EXEC_STATUS_RUNNING:
      return "running";
    case GrpcExecStatus.EXEC_STATUS_SUCCEEDED:
      return "succeeded";
    case GrpcExecStatus.EXEC_STATUS_FAILED:
      return "failed";
    case GrpcExecStatus.EXEC_STATUS_CANCELLED:
      return "cancelled";
    case GrpcExecStatus.EXEC_STATUS_TIMED_OUT:
      return "timed-out";
    default:
      throw new Error(`unknown exec status ${status}`);
  }
}

function fromGrpcServiceProtocol(protocol: GrpcServiceProtocol): ServiceDefinition["protocol"] {
  switch (protocol) {
    case GrpcServiceProtocol.SERVICE_PROTOCOL_TCP:
      return "tcp";
    case GrpcServiceProtocol.SERVICE_PROTOCOL_UDP:
      return "udp";
    default:
      throw new Error(`unknown service protocol ${protocol}`);
  }
}
