import { createChannel, createClient, Metadata, type Channel, type CallOptions } from "nice-grpc";
import {
  OperonRuntimeDefinition,
  type FsList as GrpcFsList,
  type FsStat as GrpcFsStat,
  type FsWrite as GrpcFsWrite,
  type JobEvent as GrpcJobEvent,
  type JobList as GrpcJobList,
  type JobLog as GrpcJobLog,
  type JobLogList as GrpcJobLogList,
  type JobRecord as GrpcJobRecord,
  type JobStdin as GrpcJobStdin,
  type JobStdinClose as GrpcJobStdinClose,
  type OperonRuntimeClient,
  type ServiceCheck as GrpcServiceCheck,
  type ServiceList as GrpcServiceList,
} from "./generated/operon/runtime";

export type NetworkProvider =
  | "manual"
  | "cloudflare-mesh"
  | "tailscale"
  | "wireguard"
  | "ssh"
  | "lan"
  | "kubernetes";

export type NodeEndpoint = {
  nodeId: string;
  endpoint: string;
  provider?: NetworkProvider;
  token?: string;
};

export type OperonStep = {
  id?: string;
  node: string;
  action: "fs.stat" | "fs.list" | "fs.read" | "fs.write" | "fs.copy" | "job.run";
  path?: string;
  fromPath?: string;
  toPath?: string;
  content?: string;
  command?: string;
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

export type JobRecord = {
  id: string;
  node_id: string;
  command: string;
  cwd: string;
  status: "running" | "succeeded" | "failed" | "cancelled" | "timed-out";
  exit_code?: number | null;
  log_count: number;
  logs_truncated: boolean;
};

export type JobList = {
  jobs: JobRecord[];
};

export type JobLog = {
  stream: string;
  data: Uint8Array;
  sequence: number;
};

export type JobLogList = {
  job_id: string;
  logs: JobLog[];
  truncated: boolean;
  dropped_log_count: number;
};

export type JobEvent = {
  job_id: string;
  status: JobRecord["status"];
  exit_code?: number | null;
  log_count: number;
  logs_truncated: boolean;
};

export type JobStdinResult = {
  job_id: string;
  bytes_written: number;
};

export type JobStdinCloseResult = {
  job_id: string;
  closed: boolean;
};

export type ServiceDefinition = {
  id: string;
  name: string;
  host: string;
  port: number;
  protocol: "tcp";
  description: string;
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

  async readFileStream(nodeId: string, path: string): Promise<ReadableStream<Uint8Array>> {
    const endpoint = this.endpointFor(nodeId);
    return this.readFileStreamWithEndpoint(endpoint, path);
  }

  async writeFileBytes(nodeId: string, path: string, body: BodyInit): Promise<unknown> {
    const endpoint = this.endpointFor(nodeId);
    const bytes = await bodyToBytes(body);
    return fromGrpcFsWrite(
      await this.grpcClient(endpoint).writeFile(grpcFileChunks(path, bytes), this.grpcOptions(endpoint)),
    );
  }

  async copyFile(nodeId: string, fromPath: string, toPath: string): Promise<{ from_path: string; to_path: string; bytes_copied: number }> {
    const endpoint = this.endpointFor(nodeId);
    const copy = await this.grpcClient(endpoint).copyFs({ fromPath, toPath }, this.grpcOptions(endpoint));
    return {
      from_path: copy.fromPath,
      to_path: copy.toPath,
      bytes_copied: Number(copy.bytesCopied),
    };
  }

  async listJobs(nodeId: string): Promise<JobList> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcJobList(await this.grpcClient(endpoint).listJobs({}, this.grpcOptions(endpoint)));
  }

  async listJobLogs(nodeId: string, jobId: string): Promise<JobLogList> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcJobLogList(await this.grpcClient(endpoint).listJobLogs({ jobId }, this.grpcOptions(endpoint)));
  }

  async watchJob(nodeId: string, jobId: string): Promise<AsyncIterable<JobEvent>> {
    const endpoint = this.endpointFor(nodeId);
    const events = this.grpcClient(endpoint).watchJob({ jobId }, this.grpcOptions(endpoint));
    return mapGrpcJobEvents(events);
  }

  async streamJobLogs(nodeId: string, jobId: string): Promise<ReadableStream<Uint8Array>> {
    const endpoint = this.endpointFor(nodeId);
    const iterator = this.grpcClient(endpoint).streamJobLogs({ jobId }, this.grpcOptions(endpoint))[Symbol.asyncIterator]();
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

  async writeJobStdin(nodeId: string, jobId: string, body: BodyInit): Promise<JobStdinResult> {
    const endpoint = this.endpointFor(nodeId);
    const bytes = await bodyToBytes(body);
    return fromGrpcJobStdin(
      await this.grpcClient(endpoint).writeJobStdin(grpcStdinChunks(jobId, bytes), this.grpcOptions(endpoint)),
    );
  }

  async closeJobStdin(nodeId: string, jobId: string): Promise<JobStdinCloseResult> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcJobStdinClose(
      await this.grpcClient(endpoint).closeJobStdin({ jobId }, this.grpcOptions(endpoint)),
    );
  }

  async listServices(nodeId: string): Promise<ServiceList> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcServiceList(await this.grpcClient(endpoint).listServices({}, this.grpcOptions(endpoint)));
  }

  async checkService(nodeId: string, serviceId: string): Promise<ServiceCheck> {
    const endpoint = this.endpointFor(nodeId);
    return fromGrpcServiceCheck(
      await this.grpcClient(endpoint).checkService({ serviceId }, this.grpcOptions(endpoint)),
    );
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
        return fromGrpcFsList(await client.listFs({ path: required(step.path, "path") }, options));
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
        };
      }
      case "job.run":
        return this.runGrpcJob(endpoint, step, context);
    }
  }

  private async runGrpcJob(endpoint: NodeEndpoint, step: OperonStep, context?: RequestContext): Promise<JobRecord> {
    const client = this.grpcClient(endpoint);
    const options = this.grpcOptions(endpoint, context);
    const job = fromGrpcJobRecord(
      await client.runJob(
        {
          command: required(step.command, "command"),
          cwd: step.cwd ?? "",
          timeoutSecs: String(step.timeoutSecs ?? 0),
          hasTimeoutSecs: step.timeoutSecs !== undefined,
          secrets: step.secrets ?? [],
        },
        options,
      ),
    );

    for await (const event of client.watchJob({ jobId: job.id }, options)) {
      const jobEvent = fromGrpcJobEvent(event);
      if (jobEvent.status === "running") {
        continue;
      }
      const record = fromGrpcJobRecord(await client.getJob({ jobId: job.id }, options));
      if (jobEvent.status === "succeeded") {
        return record;
      }
      throw new Error(`job ${record.id} ended with status ${jobEvent.status}`);
    }
    throw new Error(`job ${job.id} watch stream ended without a terminal event`);
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

async function* grpcFileChunks(path: string, bytes: Uint8Array): AsyncIterable<{ path: string; data: Uint8Array }> {
  if (bytes.byteLength === 0) {
    yield { path, data: new Uint8Array() };
    return;
  }
  for (let offset = 0; offset < bytes.byteLength; offset += 64 * 1024) {
    yield {
      path: offset === 0 ? path : "",
      data: bytes.subarray(offset, Math.min(offset + 64 * 1024, bytes.byteLength)),
    };
  }
}

async function* grpcStdinChunks(jobId: string, bytes: Uint8Array): AsyncIterable<{ jobId: string; data: Uint8Array }> {
  if (bytes.byteLength === 0) {
    yield { jobId, data: new Uint8Array() };
    return;
  }
  for (let offset = 0; offset < bytes.byteLength; offset += 64 * 1024) {
    yield {
      jobId: offset === 0 ? jobId : "",
      data: bytes.subarray(offset, Math.min(offset + 64 * 1024, bytes.byteLength)),
    };
  }
}

function fromGrpcFsStat(stat: GrpcFsStat) {
  return {
    path: stat.path,
    is_file: stat.isFile,
    is_dir: stat.isDir,
    size: Number(stat.size),
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
    })),
  };
}

function fromGrpcFsWrite(write: GrpcFsWrite) {
  return {
    path: write.path,
    bytes_written: Number(write.bytesWritten),
  };
}

function fromGrpcJobRecord(record: GrpcJobRecord): JobRecord {
  return {
    id: record.id,
    node_id: record.nodeId,
    command: record.command,
    cwd: record.cwd,
    status: record.status as JobRecord["status"],
    exit_code: record.hasExitCode ? record.exitCode : null,
    log_count: Number(record.logCount),
    logs_truncated: record.logsTruncated,
  };
}

function fromGrpcJobLog(log: GrpcJobLog): JobLog {
  return {
    stream: log.stream,
    data: log.data,
    sequence: Number(log.sequence),
  };
}

function fromGrpcJobLogList(list: GrpcJobLogList): JobLogList {
  return {
    job_id: list.jobId,
    logs: list.logs.map(fromGrpcJobLog),
    truncated: list.truncated,
    dropped_log_count: Number(list.droppedLogCount),
  };
}

function fromGrpcJobEvent(event: GrpcJobEvent): JobEvent {
  return {
    job_id: event.jobId,
    status: event.status as JobEvent["status"],
    exit_code: event.hasExitCode ? event.exitCode : null,
    log_count: Number(event.logCount),
    logs_truncated: event.logsTruncated,
  };
}

async function* mapGrpcJobEvents(events: AsyncIterable<GrpcJobEvent>): AsyncIterable<JobEvent> {
  for await (const event of events) {
    yield fromGrpcJobEvent(event);
  }
}

function fromGrpcJobList(list: GrpcJobList): JobList {
  return {
    jobs: list.jobs.map(fromGrpcJobRecord),
  };
}

function fromGrpcJobStdin(result: GrpcJobStdin): JobStdinResult {
  return {
    job_id: result.jobId,
    bytes_written: Number(result.bytesWritten),
  };
}

function fromGrpcJobStdinClose(result: GrpcJobStdinClose): JobStdinCloseResult {
  return {
    job_id: result.jobId,
    closed: result.closed,
  };
}

function fromGrpcServiceList(list: GrpcServiceList): ServiceList {
  return {
    services: list.services.map((service) => ({
      id: service.id,
      name: service.name,
      host: service.host,
      port: service.port,
      protocol: service.protocol as "tcp",
      description: service.description,
    })),
  };
}

function fromGrpcServiceCheck(check: GrpcServiceCheck): ServiceCheck {
  return {
    id: check.id,
    ok: check.ok,
    latency_ms: Number(check.latencyMs),
    reason: check.hasReason ? check.reason : null,
  };
}

export type { OperonRuntimeClient };
export { OperonRuntimeDefinition };
