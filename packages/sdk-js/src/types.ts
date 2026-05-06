export type NodeEndpoint = {
  nodeId: string;
  endpoint: string;
  token?: string;
};

export type OperonStep = {
  id?: string;
  node: string;
  action: "fs.stat" | "fs.list" | "fs.read" | "fs.write" | "fs.copy" | "exec.run";
  path?: string;
  fromPath?: string;
  toPath?: string;
  content?: string;
  command?: string;
  argv?: string[];
  cwd?: string;
  timeoutSecs?: number;
  secrets?: string[];
};

export type OperonRunRequest = {
  name: string;
  steps: OperonStep[];
};

export type OperonRunStatus = "running" | "succeeded" | "failed";

export type OperonTrace = {
  runId: string;
  name: string;
  status: OperonRunStatus;
  steps: OperonStepTrace[];
};

export type OperonStepTrace = {
  id: string;
  node: string;
  action: string;
  status: OperonRunStatus;
  startedAtMs: number;
  endedAtMs: number;
  error?: string;
  output?: unknown;
};

export type ExecRecord = {
  id: string;
  node_id: string;
  command: string;
  cwd: string;
  status: "running" | "succeeded" | "failed" | "cancelled" | "timed-out";
  exit_code?: number | null;
  log_count: number;
  logs_truncated: boolean;
};

export type Capability = {
  id: string;
  kind: "fs" | "process" | "exec" | "device-info" | "service" | "unspecified" | "unrecognized";
  node_id: string;
  name: string;
  permissions: string[];
  description: string;
};

export type CapabilityList = {
  capabilities: Capability[];
};

export type CapabilityDiagnosticRequest = {
  capability_id: string;
  action: string;
  resource: string;
  timeout_secs?: number;
};

export type PolicyDecision = {
  subject: string;
  capability_id: string;
  action: string;
  resource: string;
  allowed: boolean;
  reason_code: string;
  message: string;
};

export type FsStat = {
  path: string;
  is_file: boolean;
  is_dir: boolean;
  size: number;
  version: string;
};

export type FsListEntry = FsStat & {
  name: string;
};

export type FsList = {
  path: string;
  entries: FsListEntry[];
  next_page_token: string;
};

export type FsPrecondition = {
  expected_version?: string;
  require_absent?: boolean;
};

export type ExecList = {
  execs: ExecRecord[];
};

export type ExecLog = {
  stream: string;
  data: Uint8Array;
  sequence: number;
};

export type ExecLogList = {
  exec_id: string;
  logs: ExecLog[];
  truncated: boolean;
  dropped_log_count: number;
};

export type ExecLogSnapshot = ExecLogList & {
  next_sequence: number;
};

export type ExecLogStreamEvent =
  | { type: "snapshot"; snapshot: ExecLogSnapshot }
  | { type: "entry"; exec_id: string; log: ExecLog }
  | {
      type: "complete";
      exec_id: string;
      status: ExecRecord["status"];
      exit_code?: number | null;
      log_count: number;
      logs_truncated: boolean;
      truncated: boolean;
      dropped_log_count: number;
    };

export type ExecEvent = {
  exec_id: string;
  status: ExecRecord["status"];
  exit_code?: number | null;
  log_count: number;
  logs_truncated: boolean;
};

export type ExecStdinResult = {
  exec_id: string;
  bytes_written: number;
};

export type ExecStdinCloseResult = {
  exec_id: string;
  closed: boolean;
};

export type ExecSessionStart = {
  command?: string;
  argv?: string[];
  cwd?: string;
  timeoutSecs?: number;
  secrets?: string[];
  rows?: number;
  cols?: number;
};

export type ExecSessionEvent =
  | { type: "started"; exec_id: string }
  | { type: "output"; exec_id: string; data: Uint8Array }
  | { type: "exit"; exec_id: string; status: ExecRecord["status"]; exit_code?: number | null };

export type ServicePermissions = {
  check: boolean;
  forward: boolean;
};

export type ServiceDefinition = {
  id: string;
  name: string;
  host: string;
  port: number;
  protocol: "tcp" | "udp";
  description: string;
  permissions: ServicePermissions;
};

export type ServiceList = {
  services: ServiceDefinition[];
};

export type ServiceCheck = {
  id: string;
  ok: boolean;
  latency_ms: number;
  reason?: string | null;
};

export type AuditEvent = {
  subject: string;
  timestamp_ms: number;
  node_id: string;
  capability: string;
  action: string;
  resource: string;
  allowed: boolean;
  reason: string;
  run_id?: string | null;
  step_id?: string | null;
};

export type AuditLog = {
  events: AuditEvent[];
};

export type ServiceDatagram = {
  peer_id: string;
  data: Uint8Array;
};

export type ServiceDatagramTunnelEvent =
  | { type: "opened"; service_id: string; host: string; port: number }
  | { type: "datagram"; peer_id: string; data: Uint8Array }
  | { type: "close"; peer_id: string; reason: string };
