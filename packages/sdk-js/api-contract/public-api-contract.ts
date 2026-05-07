import {
  OperonClient,
  OperonRuntimeDefinition,
  type AuditEvent,
  type AuditLog,
  type Capability,
  type CapabilityDiagnosticRequest,
  type CapabilityList,
  type ExecEvent,
  type ExecList,
  type ExecLog,
  type ExecLogList,
  type ExecLogSnapshot,
  type ExecLogStreamEvent,
  type ExecRecord,
  type ExecSessionEvent,
  type ExecSessionStart,
  type ExecStdinCloseResult,
  type ExecStdinResult,
  type FsList,
  type FsListEntry,
  type FsPrecondition,
  type FsStat,
  type NodeEndpoint,
  type OperonRunRequest,
  type OperonRunStatus,
  type OperonRuntimeClient,
  type OperonStep,
  type OperonStepTrace,
  type OperonTrace,
  type PolicyDecision,
  type ServiceCheck,
  type ServiceDatagram,
  type ServiceDatagramTunnelEvent,
  type ServiceDefinition,
  type ServiceList,
  type ServicePermissions,
} from "../src/index";

type Assert<T extends true> = T;
type IsAssignable<Actual, Expected> = [Actual] extends [Expected] ? true : false;

type ClientContract = {
  close(): void;
  run(request: OperonRunRequest): Promise<OperonTrace>;
  listCapabilities(nodeId: string): Promise<CapabilityList>;
  explainCapability(
    nodeId: string,
    request: CapabilityDiagnosticRequest,
  ): Promise<PolicyDecision>;
  statFs(nodeId: string, path: string): Promise<FsStat>;
  listFs(nodeId: string, path: string): Promise<FsList>;
  readFileBytes(nodeId: string, path: string): Promise<ArrayBuffer>;
  readFileRangeBytes(
    nodeId: string,
    path: string,
    offset: number,
    size: number,
  ): Promise<Uint8Array>;
  readFileStream(nodeId: string, path: string): Promise<ReadableStream<Uint8Array>>;
  writeFileBytes(
    nodeId: string,
    path: string,
    body: BodyInit,
    precondition?: FsPrecondition,
  ): Promise<unknown>;
  copyFile(
    nodeId: string,
    fromPath: string,
    toPath: string,
  ): Promise<{ from_path: string; to_path: string; bytes_copied: number; version: string }>;
  listExecs(nodeId: string): Promise<ExecList>;
  runExec(
    nodeId: string,
    request: {
      command?: string;
      argv?: string[];
      cwd?: string;
      timeoutSecs?: number;
      secrets?: string[];
    },
  ): Promise<ExecRecord>;
  getExec(nodeId: string, execId: string): Promise<ExecRecord>;
  cancelExec(nodeId: string, execId: string): Promise<ExecRecord>;
  listExecLogs(nodeId: string, execId: string): Promise<ExecLogList>;
  watchExec(nodeId: string, execId: string): Promise<AsyncIterable<ExecEvent>>;
  streamExecLogs(nodeId: string, execId: string): Promise<ReadableStream<Uint8Array>>;
  streamExecLogEvents(
    nodeId: string,
    execId: string,
  ): Promise<AsyncIterable<ExecLogStreamEvent>>;
  writeExecStdin(nodeId: string, execId: string, body: BodyInit): Promise<ExecStdinResult>;
  closeExecStdin(nodeId: string, execId: string): Promise<ExecStdinCloseResult>;
  openExecSession(
    nodeId: string,
    start: ExecSessionStart,
    input?: AsyncIterable<Uint8Array>,
  ): Promise<AsyncIterable<ExecSessionEvent>>;
  listServices(nodeId: string): Promise<ServiceList>;
  checkService(nodeId: string, serviceId: string): Promise<ServiceCheck>;
  listAudit(nodeId: string): Promise<AuditLog>;
  openServiceTunnel(
    nodeId: string,
    serviceId: string,
    input: AsyncIterable<Uint8Array>,
  ): Promise<ReadableStream<Uint8Array>>;
  openServiceDatagramTunnel(
    nodeId: string,
    serviceId: string,
    input: AsyncIterable<ServiceDatagram>,
  ): Promise<AsyncIterable<ServiceDatagramTunnelEvent>>;
};

const endpoint: NodeEndpoint = {
  nodeId: "node-a",
  endpoint: "grpc://127.0.0.1:7789",
  token: "test-token",
};
const client: OperonClient = new OperonClient([endpoint]);
const clientContract: ClientContract = client;
clientContract.close();

const step: OperonStep = {
  node: "node-a",
  action: "exec.run",
  argv: ["echo", "hello"],
};
const runRequest: OperonRunRequest = { name: "contract", steps: [step] };
const runStatus: OperonRunStatus = "running";
const stepTrace: OperonStepTrace = {
  id: "step-1",
  node: "node-a",
  action: "exec.run",
  status: runStatus,
  startedAtMs: 1,
  endedAtMs: 2,
};
const trace: OperonTrace = {
  runId: "run-1",
  name: runRequest.name,
  status: "succeeded",
  steps: [stepTrace],
};

const capability: Capability = {
  id: "exec:default",
  kind: "exec",
  node_id: "node-a",
  name: "exec",
  permissions: ["run"],
  description: "Exec capability",
};
const capabilityList: CapabilityList = { capabilities: [capability] };
const diagnosticRequest: CapabilityDiagnosticRequest = {
  capability_id: capability.id,
  action: "run",
  resource: "/workspace",
};
const decision: PolicyDecision = {
  subject: "user",
  capability_id: capability.id,
  action: diagnosticRequest.action,
  resource: diagnosticRequest.resource,
  allowed: true,
  reason_code: "allowed",
  message: "allowed",
};

const fsStat: FsStat = {
  path: "/workspace/file.txt",
  is_file: true,
  is_dir: false,
  size: 5,
  version: "opaque",
};
const fsEntry: FsListEntry = { ...fsStat, name: "file.txt" };
const fsList: FsList = {
  path: "/workspace",
  entries: [fsEntry],
  next_page_token: "",
};
const precondition: FsPrecondition = { expected_version: fsStat.version };

const execRecord: ExecRecord = {
  id: "exec-1",
  node_id: "node-a",
  command: "echo hello",
  cwd: "/workspace",
  status: "succeeded",
  exit_code: 0,
  log_count: 1,
  logs_truncated: false,
};
const execList: ExecList = { execs: [execRecord] };
const execLog: ExecLog = { stream: "stdout", data: new Uint8Array([104]), sequence: 0 };
const execLogs: ExecLogList = {
  exec_id: execRecord.id,
  logs: [execLog],
  truncated: false,
  dropped_log_count: 0,
};
const execSnapshot: ExecLogSnapshot = {
  ...execLogs,
  next_sequence: 1,
};
const execLogEvent: ExecLogStreamEvent = {
  type: "snapshot",
  snapshot: execSnapshot,
};
const execEvent: ExecEvent = {
  exec_id: execRecord.id,
  status: execRecord.status,
  exit_code: execRecord.exit_code,
  log_count: execRecord.log_count,
  logs_truncated: execRecord.logs_truncated,
};
const stdinResult: ExecStdinResult = {
  exec_id: execRecord.id,
  bytes_written: 1,
};
const stdinCloseResult: ExecStdinCloseResult = {
  exec_id: execRecord.id,
  closed: true,
};
const sessionStart: ExecSessionStart = {
  argv: ["sh"],
  rows: 24,
  cols: 80,
};
const sessionEvent: ExecSessionEvent = {
  type: "exit",
  exec_id: execRecord.id,
  status: "succeeded",
  exit_code: 0,
};

const servicePermissions: ServicePermissions = {
  check: true,
  forward: true,
};
const service: ServiceDefinition = {
  id: "web",
  name: "web",
  host: "127.0.0.1",
  port: 8080,
  protocol: "tcp",
  description: "Local web service",
  permissions: servicePermissions,
};
const serviceList: ServiceList = { services: [service] };
const serviceCheck: ServiceCheck = {
  id: service.id,
  ok: true,
  latency_ms: 1,
};
const datagram: ServiceDatagram = {
  peer_id: "peer-1",
  data: new Uint8Array([1]),
};
const datagramEvent: ServiceDatagramTunnelEvent = {
  type: "datagram",
  peer_id: datagram.peer_id,
  data: datagram.data,
};

const auditEvent: AuditEvent = {
  subject: "user",
  timestamp_ms: 1,
  node_id: "node-a",
  capability: capability.id,
  action: "run",
  resource: "/workspace",
  allowed: true,
  reason: "allowed",
};
const auditLog: AuditLog = { events: [auditEvent] };
const runtimeDefinition = OperonRuntimeDefinition;

type _ClientContractIsStable = Assert<IsAssignable<OperonClient, ClientContract>>;
type _GeneratedClientIsPublic = Assert<IsAssignable<OperonRuntimeClient, object>>;

void [
  trace,
  capabilityList,
  decision,
  fsList,
  precondition,
  execList,
  execLogEvent,
  execEvent,
  stdinResult,
  stdinCloseResult,
  sessionStart,
  sessionEvent,
  serviceList,
  serviceCheck,
  datagramEvent,
  auditLog,
  runtimeDefinition,
];
