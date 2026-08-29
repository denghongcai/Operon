import type { CallOptions } from "nice-grpc";
import { GrpcClientPool } from "./client-transport";
import { OperonRuntimeDefinition, type OperonRuntimeClient } from "./generated/operon/runtime";
import { runGraph } from "./graph-runner";
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
  required,
  streamToBytes,
  toArrayBuffer,
  type RequestContext,
} from "./transport";
import type {
  AuditEvent,
  AuditLog,
  Capability,
  CapabilityDiagnosticRequest,
  CapabilityList,
  ExecEvent,
  ExecList,
  ExecLogList,
  ExecLogStreamEvent,
  ExecRecord,
  ExecSessionEvent,
  ExecSessionStart,
  ExecStdinCloseResult,
  ExecStdinResult,
  FsList,
  FsPrecondition,
  FsStat,
  NodeEndpoint,
  OperonRunRequest,
  OperonStep,
  OperonTrace,
  PolicyDecision,
  ServiceCheck,
  ServiceDatagram,
  ServiceDatagramTunnelEvent,
  ServiceDefinition,
  ServiceList,
} from "./types";

export type {
  AuditEvent,
  AuditLog,
  Capability,
  CapabilityDiagnosticRequest,
  CapabilityList,
  ExecEvent,
  ExecList,
  ExecLog,
  ExecLogList,
  ExecLogSnapshot,
  ExecLogStreamEvent,
  ExecRecord,
  ExecSessionEvent,
  ExecSessionStart,
  ExecStdinCloseResult,
  ExecStdinResult,
  FsList,
  FsListEntry,
  FsPrecondition,
  FsStat,
  NodeEndpoint,
  OperonRunRequest,
  OperonRunStatus,
  OperonStep,
  OperonStepTrace,
  OperonTrace,
  PolicyDecision,
  ServiceCheck,
  ServiceDatagram,
  ServiceDatagramTunnelEvent,
  ServiceDefinition,
  ServiceList,
  ServicePermissions,
} from "./types";

export class OperonClient {
  private readonly transport: GrpcClientPool;

  constructor(endpoints: NodeEndpoint[]) {
    this.transport = new GrpcClientPool(endpoints);
  }

  close(): void {
    this.transport.close();
  }

  async run(request: OperonRunRequest): Promise<OperonTrace> {
    return runGraph(request, (step, context) => this.runAction(step, context));
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

  private async runAction(step: OperonStep, context?: RequestContext): Promise<unknown> {
    const endpoint = this.endpointFor(step.node);
    return this.runGrpcAction(endpoint, step, context);
  }

  private endpointFor(nodeId: string): NodeEndpoint {
    return this.transport.endpoint(nodeId);
  }

  private grpcClient(endpoint: NodeEndpoint): OperonRuntimeClient {
    return this.transport.client(endpoint);
  }

  private grpcOptions(endpoint: NodeEndpoint, context?: RequestContext): CallOptions {
    return this.transport.options(endpoint, context);
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
