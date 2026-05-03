import { createChannel, createClient, Metadata, type Channel, type CallOptions } from "nice-grpc";
import {
  CapabilityKind as GrpcCapabilityKind,
  ExecStatus as GrpcExecStatus,
  OperonRuntimeDefinition,
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
  type ExecStdin as GrpcExecStdin,
  type ExecStdinClose as GrpcExecStdinClose,
  type OperonRuntimeClient,
  type ServiceCheck as GrpcServiceCheck,
  type ServiceDatagramTunnelResponse as GrpcServiceDatagramTunnelResponse,
  type ServiceList as GrpcServiceList,
  type ServiceTunnelResponse as GrpcServiceTunnelResponse,
} from "./generated/operon/runtime";

const DEFAULT_LIST_PAGE_SIZE = 1000;

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

type RequestContext = {
  runId?: string;
  stepId?: string;
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

export type FsStat = ReturnType<typeof fromGrpcFsStat>;

export type FsList = ReturnType<typeof fromGrpcFsList>;

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

export type ServiceDefinition = {
  id: string;
  name: string;
  host: string;
  port: number;
  protocol: "tcp" | "udp";
  description: string;
  permissions: ServicePermissions;
};

export type ServicePermissions = {
  check: boolean;
  forward: boolean;
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

export class OperonClient {
  private readonly endpoints: Map<string, NodeEndpoint>;
  private readonly grpcClients = new Map<string, { channel: Channel; client: OperonRuntimeClient }>();

  constructor(endpoints: NodeEndpoint[]) {
    this.endpoints = new Map(endpoints.map((endpoint) => [endpoint.nodeId, endpoint]));
  }

  close(): void {
    for (const { channel } of this.grpcClients.values()) {
      channel.close();
    }
    this.grpcClients.clear();
  }

  async run(request: OperonRunRequest): Promise<OperonTrace> {
    const trace: OperonTrace = {
      runId: `run-${Date.now()}`,
      name: request.name,
      status: "running",
      steps: [],
    };

    for (const [index, step] of request.steps.entries()) {
      const stepTrace = await this.runStep(step, index, trace.runId);
      trace.steps.push(stepTrace);

      if (stepTrace.status === "failed") {
        trace.status = "failed";
        return trace;
      }
    }

    trace.status = "succeeded";
    return trace;
  }

  async readFileBytes(nodeId: string, path: string): Promise<ArrayBuffer> {
    const chunks: Uint8Array[] = [];
    const stream = await this.readFileStream(nodeId, path);
    const reader = stream.getReader();
    while (true) {
      const next = await reader.read();
      if (next.done) {
        break;
      }
      chunks.push(next.value);
    }
    return toArrayBuffer(concatChunks(chunks));
  }

  async listCapabilities(nodeId: string): Promise<CapabilityList> {
    const endpoint = this.endpointFor(nodeId);
    const capabilities: Capability[] = [];
    let pageToken = "";
    do {
      const page = await this.grpcClient(endpoint).listCapabilities(
        { pageSize: DEFAULT_LIST_PAGE_SIZE, pageToken },
        this.grpcOptions(endpoint),
      );
      capabilities.push(...page.capabilities.map(fromGrpcCapability));
      pageToken = page.nextPageToken;
    } while (pageToken);
    return { capabilities };
  }

  async explainCapability(
    nodeId: string,
    request: CapabilityDiagnosticRequest,
  ): Promise<PolicyDecision> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcPolicyDecision(
      await this.grpcClient(endpoint).explainCapability(
        {
          capabilityId: request.capability_id,
          action: request.action,
          resource: request.resource,
          timeoutSecs: request.timeout_secs?.toString(),
        },
        this.grpcOptions(endpoint),
      ),
    );
  }

  async statFs(nodeId: string, path: string): Promise<FsStat> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcFsStat(
      await this.grpcClient(endpoint).statFs({ path }, this.grpcOptions(endpoint)),
    );
  }

  async listFs(nodeId: string, path: string): Promise<FsList> {
    const endpoint = this.endpointFor(nodeId);
    return this.listFsWithEndpoint(endpoint, path);
  }

  async readFileRangeBytes(
    nodeId: string,
    path: string,
    offset: number,
    size: number,
  ): Promise<Uint8Array> {
    const endpoint = this.endpointFor(nodeId);
    return this.readFileRangeBytesWithEndpoint(endpoint, path, offset, size);
  }

  async readFileStream(nodeId: string, path: string): Promise<ReadableStream<Uint8Array>> {
    const endpoint = this.endpointFor(nodeId);
    return this.readFileStreamWithEndpoint(endpoint, path);
  }

  async writeFileBytes(
    nodeId: string,
    path: string,
    body: BodyInit,
    precondition?: FsPrecondition,
  ): Promise<unknown> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcFsWrite(
      await this.grpcClient(endpoint).writeFile(
        grpcFileChunksFromBody(path, body, precondition),
        this.grpcOptions(endpoint),
      ),
    );
  }

  async copyFile(nodeId: string, fromPath: string, toPath: string): Promise<{ from_path: string; to_path: string; bytes_copied: number; version: string }> {
    const endpoint = this.endpointFor(nodeId);
    const copy = await this.grpcClient(endpoint).copyFs({ fromPath, toPath }, this.grpcOptions(endpoint));
    return {
      from_path: copy.fromPath,
      to_path: copy.toPath,
      bytes_copied: Number(copy.bytesCopied),
      version: copy.version,
    };
  }

  async listExecs(nodeId: string): Promise<ExecList> {
    const endpoint = this.endpointFor(nodeId);
    const execs: ExecRecord[] = [];
    let pageToken = "";
    do {
      const page = await this.grpcClient(endpoint).listExecs(
        { pageSize: DEFAULT_LIST_PAGE_SIZE, pageToken },
        this.grpcOptions(endpoint),
      );
      execs.push(...page.execs.map(fromGrpcExecRecord));
      pageToken = page.nextPageToken;
    } while (pageToken);
    return { execs };
  }

  async runExec(
    nodeId: string,
    request: { command?: string; argv?: string[]; cwd?: string; timeoutSecs?: number; secrets?: string[] },
  ): Promise<ExecRecord> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcExecRecord(
      await this.grpcClient(endpoint).runExec(
        {
          command: request.command ?? "",
          argv: request.argv ?? [],
          cwd: request.cwd ?? "",
          timeoutSecs: request.timeoutSecs === undefined ? undefined : String(request.timeoutSecs),
          secrets: request.secrets ?? [],
        },
        this.grpcOptions(endpoint),
      ),
    );
  }

  async getExec(nodeId: string, execId: string): Promise<ExecRecord> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcExecRecord(
      await this.grpcClient(endpoint).getExec({ execId }, this.grpcOptions(endpoint)),
    );
  }

  async cancelExec(nodeId: string, execId: string): Promise<ExecRecord> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcExecRecord(
      await this.grpcClient(endpoint).cancelExec({ execId }, this.grpcOptions(endpoint)),
    );
  }

  async listExecLogs(nodeId: string, execId: string): Promise<ExecLogList> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcExecLogList(await this.grpcClient(endpoint).listExecLogs({ execId }, this.grpcOptions(endpoint)));
  }

  async watchExec(nodeId: string, execId: string): Promise<AsyncIterable<ExecEvent>> {
    const endpoint = this.endpointFor(nodeId);
    const events = this.grpcClient(endpoint).watchExec({ execId }, this.grpcOptions(endpoint));
    return mapGrpcExecEvents(events);
  }

  async streamExecLogs(nodeId: string, execId: string): Promise<ReadableStream<Uint8Array>> {
    const iterator = streamEventLogs(await this.streamExecLogEvents(nodeId, execId))[Symbol.asyncIterator]();
    return new ReadableStream<Uint8Array>({
      async pull(controller) {
        const next = await iterator.next();
        if (next.done) {
          controller.close();
          return;
        }
        controller.enqueue(next.value);
      },
      async cancel() {
        if (iterator.return) {
          await iterator.return();
        }
      },
    });
  }

  async streamExecLogEvents(nodeId: string, execId: string): Promise<AsyncIterable<ExecLogStreamEvent>> {
    const endpoint = this.endpointFor(nodeId);
    const events = this.grpcClient(endpoint).streamExecLogs({ execId }, this.grpcOptions(endpoint));
    return mapGrpcExecLogStreamEvents(events);
  }

  async writeExecStdin(nodeId: string, execId: string, body: BodyInit): Promise<ExecStdinResult> {
    const endpoint = this.endpointFor(nodeId);
    const bytes = await bodyToBytes(body);
    return fromGrpcExecStdin(
      await this.grpcClient(endpoint).writeExecStdin(grpcStdinChunks(execId, bytes), this.grpcOptions(endpoint)),
    );
  }

  async closeExecStdin(nodeId: string, execId: string): Promise<ExecStdinCloseResult> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcExecStdinClose(
      await this.grpcClient(endpoint).closeExecStdin({ execId }, this.grpcOptions(endpoint)),
    );
  }

  async listServices(nodeId: string): Promise<ServiceList> {
    const endpoint = this.endpointFor(nodeId);
    const services: ServiceDefinition[] = [];
    let pageToken = "";
    do {
      const page = await this.grpcClient(endpoint).listServices(
        { pageSize: DEFAULT_LIST_PAGE_SIZE, pageToken },
        this.grpcOptions(endpoint),
      );
      services.push(...page.services.map(fromGrpcServiceDefinition));
      pageToken = page.nextPageToken;
    } while (pageToken);
    return { services };
  }

  async checkService(nodeId: string, serviceId: string): Promise<ServiceCheck> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcServiceCheck(
      await this.grpcClient(endpoint).checkService({ serviceId }, this.grpcOptions(endpoint)),
    );
  }

  async listAudit(nodeId: string): Promise<AuditLog> {
    const endpoint = this.endpointFor(nodeId);
    const events: AuditEvent[] = [];
    let pageToken = "";
    do {
      const page = await this.grpcClient(endpoint).listAudit(
        { pageSize: DEFAULT_LIST_PAGE_SIZE, pageToken },
        this.grpcOptions(endpoint),
      );
      events.push(...page.events.map(fromGrpcAuditEvent));
      pageToken = page.nextPageToken;
    } while (pageToken);
    return { events };
  }

  async openServiceTunnel(
    nodeId: string,
    serviceId: string,
    input: AsyncIterable<Uint8Array>,
  ): Promise<ReadableStream<Uint8Array>> {
    const endpoint = this.endpointFor(nodeId);
    const iterator = this.grpcClient(endpoint)
      .openServiceTunnel(grpcServiceTunnelRequests(serviceId, input), this.grpcOptions(endpoint))[Symbol.asyncIterator]();
    return serviceTunnelReadableStream(iterator);
  }

  async openServiceDatagramTunnel(
    nodeId: string,
    serviceId: string,
    input: AsyncIterable<ServiceDatagram>,
  ): Promise<AsyncIterable<ServiceDatagramTunnelEvent>> {
    const endpoint = this.endpointFor(nodeId);
    const responses = this.grpcClient(endpoint).openServiceDatagramTunnel(
      grpcServiceDatagramTunnelRequests(serviceId, input),
      this.grpcOptions(endpoint),
    );
    return mapGrpcServiceDatagramTunnelEvents(responses);
  }

  private async runStep(step: OperonStep, index: number, runId: string): Promise<OperonStepTrace> {
    const startedAtMs = Date.now();
    const id = step.id ?? `step-${index + 1}`;

    try {
      const output = await this.runAction(step, { runId, stepId: id });
      return {
        id,
        node: step.node,
        action: step.action,
        status: "succeeded",
        startedAtMs,
        endedAtMs: Date.now(),
        output,
      };
    } catch (error) {
      return {
        id,
        node: step.node,
        action: step.action,
        status: "failed",
        startedAtMs,
        endedAtMs: Date.now(),
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async runAction(step: OperonStep, context?: RequestContext): Promise<unknown> {
    const endpoint = this.endpointFor(step.node);
    return this.runGrpcAction(endpoint, step, context);
  }

  private endpointFor(nodeId: string): NodeEndpoint {
    const endpoint = this.endpoints.get(nodeId);
    if (!endpoint) {
      throw new Error(`node ${nodeId} not found`);
    }
    return endpoint;
  }

  private grpcClient(endpoint: NodeEndpoint): OperonRuntimeClient {
    const cached = this.grpcClients.get(endpoint.nodeId);
    if (cached) {
      return cached.client;
    }
    const channel = createChannel(grpcTarget(endpoint.endpoint));
    const client = createClient(OperonRuntimeDefinition, channel);
    this.grpcClients.set(endpoint.nodeId, { channel, client });
    return client;
  }

  private grpcOptions(endpoint: NodeEndpoint, context?: RequestContext): CallOptions {
    if (!endpoint.token && !context?.runId && !context?.stepId) {
      return {};
    }
    let metadata = Metadata();
    if (endpoint.token) {
      metadata = metadata.set("authorization", `Bearer ${endpoint.token}`);
    }
    if (context?.runId) {
      metadata = metadata.set("x-operon-run-id", context.runId);
    }
    if (context?.stepId) {
      metadata = metadata.set("x-operon-step-id", context.stepId);
    }
    return {
      metadata,
    };
  }

  private async runGrpcAction(endpoint: NodeEndpoint, step: OperonStep, context?: RequestContext): Promise<unknown> {
    const client = this.grpcClient(endpoint);
    const options = this.grpcOptions(endpoint, context);
    switch (step.action) {
      case "fs.stat":
        return fromGrpcFsStat(await client.statFs({ path: required(step.path, "path") }, options));
      case "fs.list":
        return this.listFsWithEndpoint(endpoint, required(step.path, "path"), context);
      case "fs.read": {
        return {
          path: required(step.path, "path"),
          content: new TextDecoder().decode(
            await streamToBytes(await this.readFileStreamWithEndpoint(endpoint, required(step.path, "path"), context)),
          ),
        };
      }
      case "fs.write":
        return fromGrpcFsWrite(
          await client.writeFile(
            grpcFileChunks(required(step.path, "path"), new TextEncoder().encode(step.content ?? "")),
            options,
          ),
        );
      case "fs.copy": {
        const copy = await client.copyFs(
          {
            fromPath: required(step.fromPath ?? step.path, "fromPath"),
            toPath: required(step.toPath, "toPath"),
          },
          options,
        );
        return {
          from_path: copy.fromPath,
          to_path: copy.toPath,
          bytes_copied: Number(copy.bytesCopied),
          version: copy.version,
        };
      }
      case "exec.run":
        return this.runGrpcExec(endpoint, step, context);
    }
  }

  private async runGrpcExec(endpoint: NodeEndpoint, step: OperonStep, context?: RequestContext): Promise<ExecRecord> {
    const client = this.grpcClient(endpoint);
    const options = this.grpcOptions(endpoint, context);
    const argv = step.argv ?? [];
    const exec = fromGrpcExecRecord(
      await client.runExec(
        {
          command: argv.length > 0 ? "" : required(step.command, "command"),
          argv,
          cwd: step.cwd ?? "",
          timeoutSecs: step.timeoutSecs === undefined ? undefined : String(step.timeoutSecs),
          secrets: step.secrets ?? [],
        },
        options,
      ),
    );

    for await (const event of client.watchExec({ execId: exec.id }, options)) {
      const execEvent = fromGrpcExecEvent(event);
      if (execEvent.status === "running") {
        continue;
      }
      const record = fromGrpcExecRecord(await client.getExec({ execId: exec.id }, options));
      if (execEvent.status === "succeeded") {
        return record;
      }
      throw new Error(`exec ${record.id} ended with status ${execEvent.status}`);
    }
    throw new Error(`exec ${exec.id} watch stream ended without a terminal event`);
  }

  private async readFileStreamWithEndpoint(
    endpoint: NodeEndpoint,
    path: string,
    context?: RequestContext,
  ): Promise<ReadableStream<Uint8Array>> {
    const iterator = this.grpcClient(endpoint).readFile({ path }, this.grpcOptions(endpoint, context))[Symbol.asyncIterator]();
    return new ReadableStream<Uint8Array>({
      async pull(controller) {
        const next = await iterator.next();
        if (next.done) {
          controller.close();
          return;
        }
        controller.enqueue(next.value.data);
      },
      async cancel() {
        if (iterator.return) {
          await iterator.return();
        }
      },
    });
  }

  private async readFileRangeBytesWithEndpoint(
    endpoint: NodeEndpoint,
    path: string,
    offset: number,
    size: number,
    context?: RequestContext,
  ): Promise<Uint8Array> {
    const response = await this.grpcClient(endpoint).readFileRange(
      { path, offset: String(offset), size },
      this.grpcOptions(endpoint, context),
    );
    return response.data;
  }

  private async listFsWithEndpoint(
    endpoint: NodeEndpoint,
    path: string,
    context?: RequestContext,
  ): Promise<FsList> {
    const entries: ReturnType<typeof fromGrpcFsList>["entries"] = [];
    let pageToken = "";
    do {
      const page = await this.grpcClient(endpoint).listFs(
        { path, pageSize: DEFAULT_LIST_PAGE_SIZE, pageToken },
        this.grpcOptions(endpoint, context),
      );
      entries.push(...fromGrpcFsList(page).entries);
      pageToken = page.nextPageToken;
    } while (pageToken);
    return { path, entries, next_page_token: "" };
  }
}

function required(value: string | undefined, field: string): string {
  if (!value) {
    throw new Error(`step requires ${field}`);
  }
  return value;
}

function grpcTarget(endpoint: string): string {
  if (endpoint.startsWith("grpc://")) {
    return `http://${endpoint.slice("grpc://".length)}`;
  }
  if (endpoint.startsWith("grpcs://")) {
    return `https://${endpoint.slice("grpcs://".length)}`;
  }
  throw new Error(`endpoint ${endpoint} is not a gRPC endpoint`);
}

async function bodyToBytes(body: BodyInit): Promise<Uint8Array> {
  if (typeof body === "string") {
    return new TextEncoder().encode(body);
  }
  if (body instanceof ArrayBuffer) {
    return new Uint8Array(body);
  }
  if (ArrayBuffer.isView(body)) {
    return new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
  }
  if (body instanceof Blob) {
    return new Uint8Array(await body.arrayBuffer());
  }
  if (body instanceof URLSearchParams) {
    return new TextEncoder().encode(body.toString());
  }
  if (body instanceof ReadableStream) {
    const reader = body.getReader();
    const chunks: Uint8Array[] = [];
    while (true) {
      const next = await reader.read();
      if (next.done) {
        break;
      }
      chunks.push(next.value);
    }
    return concatChunks(chunks);
  }
  throw new Error("unsupported BodyInit for gRPC streaming request");
}

async function* bodyToByteChunks(body: BodyInit): AsyncIterable<Uint8Array> {
  if (body instanceof ReadableStream) {
    const reader = body.getReader();
    while (true) {
      const next = await reader.read();
      if (next.done) {
        return;
      }
      yield next.value;
    }
  } else {
    yield await bodyToBytes(body);
  }
}

function concatChunks(chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0);
  const merged = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return merged;
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const buffer = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(buffer).set(bytes);
  return buffer;
}

async function streamToBytes(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  while (true) {
    const next = await reader.read();
    if (next.done) {
      break;
    }
    chunks.push(next.value);
  }
  return concatChunks(chunks);
}

async function* grpcFileChunks(
  path: string,
  bytes: Uint8Array,
  precondition?: FsPrecondition,
): AsyncIterable<{
  target?: ReturnType<typeof grpcFileTarget>;
  chunk?: { data: Uint8Array };
}> {
  yield { target: grpcFileTarget(path, precondition) };
  if (bytes.byteLength === 0) {
    yield { chunk: { data: new Uint8Array() } };
    return;
  }
  for (let offset = 0; offset < bytes.byteLength; offset += 64 * 1024) {
    yield {
      chunk: { data: bytes.subarray(offset, Math.min(offset + 64 * 1024, bytes.byteLength)) },
    };
  }
}

async function* grpcFileChunksFromBody(
  path: string,
  body: BodyInit,
  precondition?: FsPrecondition,
): AsyncIterable<{
  target?: ReturnType<typeof grpcFileTarget>;
  chunk?: { data: Uint8Array };
}> {
  yield { target: grpcFileTarget(path, precondition) };
  let emitted = false;
  for await (const bytes of bodyToByteChunks(body)) {
    if (bytes.byteLength === 0) {
      continue;
    }
    emitted = true;
    for (let offset = 0; offset < bytes.byteLength; offset += 64 * 1024) {
      yield {
        chunk: { data: bytes.subarray(offset, Math.min(offset + 64 * 1024, bytes.byteLength)) },
      };
    }
  }
  if (!emitted) {
    yield { chunk: { data: new Uint8Array() } };
  }
}

function grpcFileTarget(path: string, precondition?: FsPrecondition) {
  const requireAbsent = precondition?.require_absent ?? false;
  const expectedVersion = precondition?.expected_version;
  return {
    path,
    precondition:
      expectedVersion || requireAbsent
        ? { expectedVersion, requireAbsent }
        : undefined,
    expectedVersion,
    requireAbsent,
  };
}

async function* grpcStdinChunks(execId: string, bytes: Uint8Array): AsyncIterable<{ target?: { execId: string }; chunk?: { data: Uint8Array } }> {
  yield { target: { execId } };
  if (bytes.byteLength === 0) {
    yield { chunk: { data: new Uint8Array() } };
    return;
  }
  for (let offset = 0; offset < bytes.byteLength; offset += 64 * 1024) {
    yield {
      chunk: { data: bytes.subarray(offset, Math.min(offset + 64 * 1024, bytes.byteLength)) },
    };
  }
}

async function* grpcServiceTunnelRequests(
  serviceId: string,
  input: AsyncIterable<Uint8Array>,
): AsyncIterable<{ target?: { serviceId: string }; data?: { data: Uint8Array }; close?: { reason: string } }> {
  yield { target: { serviceId } };
  for await (const chunk of input) {
    yield { data: { data: chunk } };
  }
  yield { close: { reason: "client input ended" } };
}

async function* grpcServiceDatagramTunnelRequests(
  serviceId: string,
  input: AsyncIterable<ServiceDatagram>,
): AsyncIterable<{
  target?: { serviceId: string };
  datagram?: { peerId: string; data: Uint8Array };
  close?: { peerId: string; reason: string };
}> {
  yield { target: { serviceId } };
  for await (const datagram of input) {
    yield { datagram: { peerId: datagram.peer_id, data: datagram.data } };
  }
  yield { close: { peerId: "", reason: "client input ended" } };
}

function serviceTunnelReadableStream(
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

async function* mapGrpcServiceDatagramTunnelEvents(
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

function fromGrpcFsStat(stat: GrpcFsStat) {
  return {
    path: stat.path,
    is_file: stat.isFile,
    is_dir: stat.isDir,
    size: Number(stat.size),
    version: stat.version,
  };
}

function fromGrpcFsList(list: GrpcFsList) {
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

function fromGrpcCapability(capability: GrpcCapability): Capability {
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

function fromGrpcPolicyDecision(decision: GrpcPolicyDecision): PolicyDecision {
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

function fromGrpcFsWrite(write: GrpcFsWrite) {
  return {
    path: write.path,
    bytes_written: Number(write.bytesWritten),
    version: write.version,
  };
}

function fromGrpcExecRecord(record: GrpcExecRecord): ExecRecord {
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

function fromGrpcExecLogList(list: GrpcExecLogList): ExecLogList {
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

function fromGrpcExecEvent(event: GrpcExecEvent): ExecEvent {
  return {
    exec_id: event.execId,
    status: fromGrpcExecStatus(event.status),
    exit_code: event.exitCode ?? null,
    log_count: Number(event.logCount),
    logs_truncated: event.logsTruncated,
  };
}

function fromGrpcAuditEvent(event: GrpcAuditLog["events"][number]): AuditEvent {
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

async function* mapGrpcExecEvents(events: AsyncIterable<GrpcExecEvent>): AsyncIterable<ExecEvent> {
  for await (const event of events) {
    yield fromGrpcExecEvent(event);
  }
}

async function* mapGrpcExecLogStreamEvents(events: AsyncIterable<GrpcExecLogStreamEvent>): AsyncIterable<ExecLogStreamEvent> {
  for await (const event of events) {
    const mapped = fromGrpcExecLogStreamEvent(event);
    if (mapped) {
      yield mapped;
    }
  }
}

async function* streamEventLogs(events: AsyncIterable<ExecLogStreamEvent>): AsyncIterable<Uint8Array> {
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

function fromGrpcExecList(list: GrpcExecList): ExecList {
  return {
    execs: list.execs.map(fromGrpcExecRecord),
  };
}

function fromGrpcExecStdin(result: GrpcExecStdin): ExecStdinResult {
  return {
    exec_id: result.execId,
    bytes_written: Number(result.bytesWritten),
  };
}

function fromGrpcExecStdinClose(result: GrpcExecStdinClose): ExecStdinCloseResult {
  return {
    exec_id: result.execId,
    closed: result.closed,
  };
}

function fromGrpcServiceList(list: GrpcServiceList): ServiceList {
  return {
    services: list.services.map(fromGrpcServiceDefinition),
  };
}

function fromGrpcServiceDefinition(service: GrpcServiceList["services"][number]): ServiceDefinition {
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

function fromGrpcServiceCheck(check: GrpcServiceCheck): ServiceCheck {
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

export type { OperonRuntimeClient };
export { OperonRuntimeDefinition };
