import { createChannel, createClient, Metadata, type Channel, type CallOptions } from "nice-grpc";
import {
  OperonRuntimeDefinition,
  type CapabilityList as GrpcCapabilityList,
  type FsList as GrpcFsList,
  type FsStat as GrpcFsStat,
  type FsWrite as GrpcFsWrite,
  type JobList as GrpcJobList,
  type JobLog as GrpcJobLog,
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
  action: "fs.stat" | "fs.list" | "fs.read" | "fs.write" | "job.run";
  path?: string;
  content?: string;
  command?: string;
  cwd?: string;
  timeoutSecs?: number;
  secrets?: string[];
};

export type OperonErrorResponse = {
  code: string;
  message: string;
  status: number;
  capability?: string;
  resource?: string;
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

export type JobRecord = {
  id: string;
  node_id: string;
  command: string;
  cwd: string;
  status: "running" | "succeeded" | "failed" | "cancelled" | "timed-out";
  exit_code?: number | null;
  logs: Array<{ stream: string; data: string }>;
};

export type JobList = {
  jobs: JobRecord[];
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
      const stepTrace = await this.runStep(step, index);
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
    const endpoint = this.endpointFor(nodeId);
    if (isGrpcEndpoint(endpoint)) {
      const chunks: Uint8Array[] = [];
      for await (const chunk of this.grpcClient(endpoint).readFile({ path }, this.grpcOptions(endpoint))) {
        chunks.push(chunk.data);
      }
      return toArrayBuffer(concatChunks(chunks));
    }
    return this.request<ArrayBuffer>(nodeId, `/fs/read-stream?path=${encodeURIComponent(path)}`, {
      method: "GET",
      headers: { accept: "application/octet-stream" },
    });
  }

  async writeFileBytes(nodeId: string, path: string, body: BodyInit): Promise<unknown> {
    const endpoint = this.endpointFor(nodeId);
    if (isGrpcEndpoint(endpoint)) {
      const bytes = await bodyToBytes(body);
      return grpcFsWriteToHttpShape(
        await this.grpcClient(endpoint).writeFile(grpcFileChunks(path, bytes), this.grpcOptions(endpoint)),
      );
    }
    return this.request(nodeId, `/fs/write-stream?path=${encodeURIComponent(path)}`, {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body,
    });
  }

  async listJobs(nodeId: string): Promise<JobList> {
    const endpoint = this.endpointFor(nodeId);
    if (isGrpcEndpoint(endpoint)) {
      return grpcJobListToHttpShape(await this.grpcClient(endpoint).listJobs({}, this.grpcOptions(endpoint)));
    }
    return this.get<JobList>(nodeId, "/job/list");
  }

  async streamJobLogs(nodeId: string, jobId: string): Promise<ReadableStream<Uint8Array>> {
    const endpoint = this.endpointFor(nodeId);
    if (isGrpcEndpoint(endpoint)) {
      const iterator = this.grpcClient(endpoint).streamJobLogs({ jobId }, this.grpcOptions(endpoint))[Symbol.asyncIterator]();
      return new ReadableStream<Uint8Array>({
        async pull(controller) {
          const next = await iterator.next();
          if (next.done) {
            controller.close();
            return;
          }
          controller.enqueue(new TextEncoder().encode(next.value.data));
        },
        async cancel() {
          if (iterator.return) {
            await iterator.return();
          }
        },
      });
    }
    const response = await this.fetchRaw(nodeId, `/job/logs-stream?id=${encodeURIComponent(jobId)}`, {
      method: "GET",
      headers: { accept: "application/octet-stream" },
    });
    if (!response.body) {
      throw new Error("job logs stream response has no body");
    }
    return response.body;
  }

  async writeJobStdin(nodeId: string, jobId: string, body: BodyInit): Promise<JobStdinResult> {
    const endpoint = this.endpointFor(nodeId);
    if (isGrpcEndpoint(endpoint)) {
      const bytes = await bodyToBytes(body);
      return grpcJobStdinToHttpShape(
        await this.grpcClient(endpoint).writeJobStdin(grpcStdinChunks(jobId, bytes), this.grpcOptions(endpoint)),
      );
    }
    return this.request<JobStdinResult>(nodeId, `/job/stdin?id=${encodeURIComponent(jobId)}`, {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body,
    });
  }

  async closeJobStdin(nodeId: string, jobId: string): Promise<JobStdinCloseResult> {
    const endpoint = this.endpointFor(nodeId);
    if (isGrpcEndpoint(endpoint)) {
      return grpcJobStdinCloseToHttpShape(
        await this.grpcClient(endpoint).closeJobStdin({ jobId }, this.grpcOptions(endpoint)),
      );
    }
    return this.post<JobStdinCloseResult>(nodeId, `/job/stdin/close?id=${encodeURIComponent(jobId)}`, {});
  }

  async listServices(nodeId: string): Promise<ServiceList> {
    const endpoint = this.endpointFor(nodeId);
    if (isGrpcEndpoint(endpoint)) {
      return grpcServiceListToHttpShape(await this.grpcClient(endpoint).listServices({}, this.grpcOptions(endpoint)));
    }
    return this.get<ServiceList>(nodeId, "/service/list");
  }

  async checkService(nodeId: string, serviceId: string): Promise<ServiceCheck> {
    const endpoint = this.endpointFor(nodeId);
    if (isGrpcEndpoint(endpoint)) {
      return grpcServiceCheckToHttpShape(
        await this.grpcClient(endpoint).checkService({ serviceId }, this.grpcOptions(endpoint)),
      );
    }
    return this.get<ServiceCheck>(nodeId, `/service/check?id=${encodeURIComponent(serviceId)}`);
  }

  private async runStep(step: OperonStep, index: number): Promise<OperonStepTrace> {
    const startedAtMs = Date.now();
    const id = step.id ?? `step-${index + 1}`;

    try {
      const output = await this.runAction(step);
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

  private async runAction(step: OperonStep): Promise<unknown> {
    const endpoint = this.endpointFor(step.node);
    if (isGrpcEndpoint(endpoint)) {
      return this.runGrpcAction(endpoint, step);
    }
    switch (step.action) {
      case "fs.stat":
        return this.get(step.node, `/fs/stat?path=${encodeURIComponent(required(step.path, "path"))}`);
      case "fs.list":
        return this.get(step.node, `/fs/list?path=${encodeURIComponent(required(step.path, "path"))}`);
      case "fs.read":
        return this.get(step.node, `/fs/read?path=${encodeURIComponent(required(step.path, "path"))}`);
      case "fs.write":
        return this.post(step.node, "/fs/write", {
          path: required(step.path, "path"),
          content: step.content ?? "",
        });
      case "job.run":
        return this.runJob(step);
    }
  }

  private async runJob(step: OperonStep): Promise<JobRecord> {
    const endpoint = this.endpointFor(step.node);
    if (isGrpcEndpoint(endpoint)) {
      return this.runGrpcJob(endpoint, step);
    }
    const job = await this.post<JobRecord>(step.node, "/job/run", {
      command: required(step.command, "command"),
      cwd: step.cwd,
      timeout_secs: step.timeoutSecs,
      secrets: step.secrets ?? [],
    });

    while (true) {
      const record = await this.get<JobRecord>(step.node, `/job/status?id=${encodeURIComponent(job.id)}`);
      if (record.status === "running") {
        await sleep(100);
        continue;
      }
      if (record.status === "succeeded") {
        return record;
      }
      throw new Error(`job ${record.id} ended with status ${record.status}`);
    }
  }

  private async get<T = unknown>(nodeId: string, path: string): Promise<T> {
    return this.request<T>(nodeId, path, { method: "GET" });
  }

  private async post<T = unknown>(nodeId: string, path: string, body: unknown): Promise<T> {
    return this.request<T>(nodeId, path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  private async request<T>(nodeId: string, path: string, init: RequestInit): Promise<T> {
    const { endpoint, response } = await this.fetchEndpoint(nodeId, path, init);
    if (!response.ok) {
      throw new Error(`request to ${endpoint.endpoint}${path} failed: ${response.status} ${response.statusText}: ${await errorMessage(response)}`);
    }
    if (response.headers.get("content-type")?.startsWith("application/json") ?? false) {
      return response.json() as Promise<T>;
    }
    return response.arrayBuffer() as Promise<T>;
  }

  private async fetchRaw(nodeId: string, path: string, init: RequestInit): Promise<Response> {
    const { endpoint, response } = await this.fetchEndpoint(nodeId, path, init);
    if (!response.ok) {
      throw new Error(`request to ${endpoint.endpoint}${path} failed: ${response.status} ${response.statusText}: ${await errorMessage(response)}`);
    }
    return response;
  }

  private async fetchEndpoint(
    nodeId: string,
    path: string,
    init: RequestInit,
  ): Promise<{ endpoint: NodeEndpoint; response: Response }> {
    const endpoint = this.endpointFor(nodeId);

    const headers = new Headers(init.headers);
    if (endpoint.token) {
      headers.set("authorization", `Bearer ${endpoint.token}`);
    }

    const response = await fetch(new URL(path, endpoint.endpoint), { ...init, headers });
    return { endpoint, response };
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

  private grpcOptions(endpoint: NodeEndpoint): CallOptions {
    if (!endpoint.token) {
      return {};
    }
    return {
      metadata: Metadata().set("authorization", `Bearer ${endpoint.token}`),
    };
  }

  private async runGrpcAction(endpoint: NodeEndpoint, step: OperonStep): Promise<unknown> {
    const client = this.grpcClient(endpoint);
    const options = this.grpcOptions(endpoint);
    switch (step.action) {
      case "fs.stat":
        return grpcFsStatToHttpShape(await client.statFs({ path: required(step.path, "path") }, options));
      case "fs.list":
        return grpcFsListToHttpShape(await client.listFs({ path: required(step.path, "path") }, options));
      case "fs.read": {
        const chunks: Uint8Array[] = [];
        for await (const chunk of client.readFile({ path: required(step.path, "path") }, options)) {
          chunks.push(chunk.data);
        }
        return { path: required(step.path, "path"), content: new TextDecoder().decode(concatChunks(chunks)) };
      }
      case "fs.write":
        return grpcFsWriteToHttpShape(
          await client.writeFile(
            grpcFileChunks(required(step.path, "path"), new TextEncoder().encode(step.content ?? "")),
            options,
          ),
        );
      case "job.run":
        return this.runGrpcJob(endpoint, step);
    }
  }

  private async runGrpcJob(endpoint: NodeEndpoint, step: OperonStep): Promise<JobRecord> {
    const client = this.grpcClient(endpoint);
    const options = this.grpcOptions(endpoint);
    const job = grpcJobRecordToHttpShape(
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

    while (true) {
      const record = grpcJobRecordToHttpShape(await client.getJob({ jobId: job.id }, options));
      if (record.status === "running") {
        await sleep(100);
        continue;
      }
      if (record.status === "succeeded") {
        return record;
      }
      throw new Error(`job ${record.id} ended with status ${record.status}`);
    }
  }
}

async function errorMessage(response: Response): Promise<string> {
  const text = await response.text();
  try {
    const error = JSON.parse(text) as OperonErrorResponse;
    return `${error.code}: ${error.message}`;
  } catch {
    return text.trim();
  }
}

function required(value: string | undefined, field: string): string {
  if (!value) {
    throw new Error(`step requires ${field}`);
  }
  return value;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function isGrpcEndpoint(endpoint: NodeEndpoint): boolean {
  return endpoint.endpoint.startsWith("grpc://") || endpoint.endpoint.startsWith("grpcs://");
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

function grpcFsStatToHttpShape(stat: GrpcFsStat) {
  return {
    path: stat.path,
    is_file: stat.isFile,
    is_dir: stat.isDir,
    size: Number(stat.size),
  };
}

function grpcFsListToHttpShape(list: GrpcFsList) {
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

function grpcFsWriteToHttpShape(write: GrpcFsWrite) {
  return {
    path: write.path,
    bytes_written: Number(write.bytesWritten),
  };
}

function grpcJobRecordToHttpShape(record: GrpcJobRecord): JobRecord {
  return {
    id: record.id,
    node_id: record.nodeId,
    command: record.command,
    cwd: record.cwd,
    status: record.status as JobRecord["status"],
    exit_code: record.hasExitCode ? record.exitCode : null,
    logs: record.logs.map(grpcJobLogToHttpShape),
  };
}

function grpcJobLogToHttpShape(log: GrpcJobLog): { stream: string; data: string } {
  return {
    stream: log.stream,
    data: log.data,
  };
}

function grpcJobListToHttpShape(list: GrpcJobList): JobList {
  return {
    jobs: list.jobs.map(grpcJobRecordToHttpShape),
  };
}

function grpcJobStdinToHttpShape(result: GrpcJobStdin): JobStdinResult {
  return {
    job_id: result.jobId,
    bytes_written: Number(result.bytesWritten),
  };
}

function grpcJobStdinCloseToHttpShape(result: GrpcJobStdinClose): JobStdinCloseResult {
  return {
    job_id: result.jobId,
    closed: result.closed,
  };
}

function grpcServiceListToHttpShape(list: GrpcServiceList): ServiceList {
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

function grpcServiceCheckToHttpShape(check: GrpcServiceCheck): ServiceCheck {
  return {
    id: check.id,
    ok: check.ok,
    latency_ms: Number(check.latencyMs),
    reason: check.hasReason ? check.reason : null,
  };
}

export type { OperonRuntimeClient };
export { OperonRuntimeDefinition };
