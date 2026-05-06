import { createChannel, createClient, type CallOptions, type Channel } from "nice-grpc";
import { OperonRuntimeDefinition, type OperonRuntimeClient } from "./generated/operon/runtime";
import {
  fromGrpcAuditEvent,
  fromGrpcCapability,
  fromGrpcExecEvent,
  fromGrpcExecList,
  fromGrpcExecLogList,
  fromGrpcExecRecord,
  fromGrpcExecStdin,
  fromGrpcExecStdinClose,
  fromGrpcFsList,
  fromGrpcFsStat,
  fromGrpcFsWrite,
  fromGrpcPolicyDecision,
  fromGrpcServiceCheck,
  fromGrpcServiceDefinition,
  fromGrpcServiceList,
  mapGrpcServiceDatagramTunnelEvents,
  mapGrpcExecEvents,
  mapGrpcExecLogStreamEvents,
  mapGrpcExecSessionEvents,
  serviceTunnelReadableStream,
  streamEventLogs,
} from "./grpc-mappers";
import {
  emptyAsyncIterable,
  grpcExecSessionRequests,
  grpcFileChunks,
  grpcFileChunksFromBody,
  grpcServiceDatagramTunnelRequests,
  grpcServiceTunnelRequests,
  grpcStdinChunks,
} from "./grpc-requests";
import {
  bodyToBytes,
  concatChunks,
  DEFAULT_LIST_PAGE_SIZE,
  grpcOptions,
  grpcTarget,
  required,
  streamToBytes,
  toArrayBuffer,
  type RequestContext,
} from "./transport";

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

  async openExecSession(
    nodeId: string,
    start: ExecSessionStart,
    input?: AsyncIterable<Uint8Array>,
  ): Promise<AsyncIterable<ExecSessionEvent>> {
    const endpoint = this.endpointFor(nodeId);
    const events = this.grpcClient(endpoint).openExecSession(
      grpcExecSessionRequests(start, input ?? emptyAsyncIterable()),
      this.grpcOptions(endpoint),
    );
    return mapGrpcExecSessionEvents(events);
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
    return grpcOptions(endpoint, context);
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

export type { OperonRuntimeClient };
export { OperonRuntimeDefinition };
