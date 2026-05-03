import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CapabilityKind, ExecStatus, ServiceProtocol } from "./generated/operon/runtime";
import { OperonClient } from "./index";

const niceGrpcMock = vi.hoisted(() => {
  const metadata = {
    set: vi.fn(function set(this: unknown) {
      return this;
    }),
  };
  return {
    channel: { close: vi.fn() },
    client: {
      statFs: vi.fn(),
      listFs: vi.fn(),
      listCapabilities: vi.fn(),
      readFile: vi.fn(),
      readFileRange: vi.fn(),
      writeFile: vi.fn(),
      copyFs: vi.fn(),
      runExec: vi.fn(),
      getExec: vi.fn(),
      listExecs: vi.fn(),
      watchExec: vi.fn(),
      listExecLogs: vi.fn(),
      streamExecLogs: vi.fn(),
      writeExecStdin: vi.fn(),
      closeExecStdin: vi.fn(),
      openExecSession: vi.fn(),
      listServices: vi.fn(),
      checkService: vi.fn(),
      cancelExec: vi.fn(),
      listAudit: vi.fn(),
      openServiceTunnel: vi.fn(),
      openServiceDatagramTunnel: vi.fn(),
      explainCapability: vi.fn(),
    },
    metadata,
    createChannel: vi.fn(),
    createClient: vi.fn(),
    Metadata: vi.fn(),
  };
});

vi.mock("nice-grpc", () => ({
  createChannel: niceGrpcMock.createChannel,
  createClient: niceGrpcMock.createClient,
  Metadata: niceGrpcMock.Metadata,
}));

beforeEach(() => {
  niceGrpcMock.createChannel.mockReturnValue(niceGrpcMock.channel);
  niceGrpcMock.createClient.mockReturnValue(niceGrpcMock.client);
  niceGrpcMock.Metadata.mockReturnValue(niceGrpcMock.metadata);
  niceGrpcMock.metadata.set.mockClear();
  niceGrpcMock.channel.close.mockClear();
  for (const mock of Object.values(niceGrpcMock.client)) {
    mock.mockReset();
  }
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("OperonClient", () => {
  it("runs fs and exec steps sequentially over gRPC and returns a successful trace", async () => {
    niceGrpcMock.client.writeFile.mockResolvedValue({ path: "/input.txt", bytesWritten: 5 });
    niceGrpcMock.client.runExec.mockResolvedValue({
      id: "exec-1",
      nodeId: "node-a",
      command: "cat input.txt",
      cwd: "/",
      status: ExecStatus.EXEC_STATUS_RUNNING,
      logCount: "0",
      logsTruncated: false,
    });
    niceGrpcMock.client.watchExec.mockReturnValue(asyncIterable([
      {
        execId: "exec-1",
        status: ExecStatus.EXEC_STATUS_RUNNING,
        logCount: "0",
        logsTruncated: false,
      },
      {
        execId: "exec-1",
        status: ExecStatus.EXEC_STATUS_SUCCEEDED,
        exitCode: 0,
        logCount: "1",
        logsTruncated: false,
      },
    ]));
    niceGrpcMock.client.getExec.mockResolvedValueOnce({
      id: "exec-1",
      nodeId: "node-a",
      command: "cat input.txt",
      cwd: "/",
      status: ExecStatus.EXEC_STATUS_SUCCEEDED,
      exitCode: 0,
      logCount: "1",
      logsTruncated: false,
    });
    niceGrpcMock.client.readFile.mockReturnValue(asyncIterable([{ data: new TextEncoder().encode("hello") }]));

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789", token: "test-token" }]);
    const trace = await client.run({
      name: "copy-and-run",
      steps: [
        { id: "write", node: "node-a", action: "fs.write", path: "/input.txt", content: "hello" },
        { id: "run", node: "node-a", action: "exec.run", command: "cat input.txt", secrets: ["GITHUB_TOKEN"] },
        { id: "read", node: "node-a", action: "fs.read", path: "/output.txt" },
      ],
    });

    expect(trace.status).toBe("succeeded");
    expect(trace.steps.map((step) => step.id)).toEqual(["write", "run", "read"]);
    expect(niceGrpcMock.createChannel).toHaveBeenCalledWith("http://127.0.0.1:7789");
    expect(niceGrpcMock.metadata.set).toHaveBeenCalledWith("authorization", "Bearer test-token");
    expect(niceGrpcMock.metadata.set).toHaveBeenCalledWith(expect.stringMatching(/^x-operon-run-id$/), expect.stringMatching(/^run-/));
    expect(niceGrpcMock.metadata.set).toHaveBeenCalledWith("x-operon-step-id", "write");
    expect(niceGrpcMock.metadata.set).toHaveBeenCalledWith("x-operon-step-id", "run");
    expect(niceGrpcMock.metadata.set).toHaveBeenCalledWith("x-operon-step-id", "read");
    expect(niceGrpcMock.client.runExec).toHaveBeenCalledWith(
      expect.objectContaining({ command: "cat input.txt", secrets: ["GITHUB_TOKEN"] }),
      expect.any(Object),
    );
  });

  it("stops on the first failed step and returns a failed trace", async () => {
    niceGrpcMock.client.readFile.mockImplementation(() => {
      throw new Error("forbidden: fs read denied by policy");
    });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const trace = await client.run({
      name: "denied",
      steps: [
        { id: "denied", node: "node-a", action: "fs.read", path: "/secret.txt" },
        { id: "skipped", node: "node-a", action: "fs.read", path: "/next.txt" },
      ],
    });

    expect(trace.status).toBe("failed");
    expect(trace.steps).toHaveLength(1);
    expect(trace.steps[0].id).toBe("denied");
    expect(trace.steps[0].error).toContain("forbidden: fs read denied by policy");
  });

  it("fails a step when a referenced node is missing", async () => {
    const client = new OperonClient([]);
    const trace = await client.run({
      name: "missing-node",
      steps: [{ node: "node-a", action: "fs.list", path: "/" }],
    });

    expect(trace.status).toBe("failed");
    expect(trace.steps[0].id).toBe("step-1");
    expect(trace.steps[0].error).toBe("node node-a not found");
  });

  it("lists and checks configured services over gRPC", async () => {
    niceGrpcMock.client.listServices.mockResolvedValueOnce({
      services: [
        {
          id: "daemon",
          name: "daemon",
          host: "127.0.0.1",
          port: 7789,
          protocol: ServiceProtocol.SERVICE_PROTOCOL_TCP,
          description: "Operon gRPC daemon",
          permissions: { check: true, forward: true },
        },
      ],
      nextPageToken: "1",
    }).mockResolvedValueOnce({
      services: [
        {
          id: "app",
          name: "app",
          host: "127.0.0.1",
          port: 3000,
          protocol: ServiceProtocol.SERVICE_PROTOCOL_UDP,
          description: "application",
          permissions: { check: true, forward: false },
        },
      ],
      nextPageToken: "",
    });
    niceGrpcMock.client.checkService.mockResolvedValue({ id: "daemon", ok: true, latencyMs: 2 });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789", token: "test-token" }]);
    const services = await client.listServices("node-a");
    const check = await client.checkService("node-a", "daemon");

    expect(services.services[0].id).toBe("daemon");
    expect(services.services[1].id).toBe("app");
    expect(services.services[1].protocol).toBe("udp");
    expect(services.services[0].port).toBe(7789);
    expect(services.services[0].permissions.forward).toBe(true);
    expect(services.services[1].permissions.forward).toBe(false);
    expect(check.ok).toBe(true);
    expect(niceGrpcMock.client.listServices).toHaveBeenCalledWith({ pageSize: 1000, pageToken: "" }, expect.any(Object));
    expect(niceGrpcMock.client.listServices).toHaveBeenCalledWith({ pageSize: 1000, pageToken: "1" }, expect.any(Object));
    expect(niceGrpcMock.client.checkService).toHaveBeenCalledWith({ serviceId: "daemon" }, expect.any(Object));
  });

  it("exposes direct public runtime APIs beyond graph helpers", async () => {
    niceGrpcMock.client.listCapabilities.mockResolvedValueOnce({
      capabilities: [{
        id: "fs:workspace",
        kind: CapabilityKind.CAPABILITY_KIND_FS,
        nodeId: "node-a",
        name: "workspace",
        permissions: ["read"],
        description: "workspace fs",
      }],
      nextPageToken: "",
    });
    niceGrpcMock.client.statFs.mockResolvedValue({ path: "/a.txt", isFile: true, isDir: false, size: "3" });
    niceGrpcMock.client.listFs.mockResolvedValue({ path: "/", entries: [], nextPageToken: "" });
    niceGrpcMock.client.runExec.mockResolvedValue({
      id: "exec-1",
      nodeId: "node-a",
      command: "true",
      cwd: "/",
      status: ExecStatus.EXEC_STATUS_RUNNING,
      logCount: "0",
      logsTruncated: false,
    });
    niceGrpcMock.client.getExec.mockResolvedValue({
      id: "exec-1",
      nodeId: "node-a",
      command: "true",
      cwd: "/",
      status: ExecStatus.EXEC_STATUS_SUCCEEDED,
      exitCode: 0,
      logCount: "0",
      logsTruncated: false,
    });
    niceGrpcMock.client.cancelExec.mockResolvedValue({
      id: "exec-1",
      nodeId: "node-a",
      command: "true",
      cwd: "/",
      status: ExecStatus.EXEC_STATUS_CANCELLED,
      logCount: "0",
      logsTruncated: false,
    });
    niceGrpcMock.client.listAudit.mockResolvedValueOnce({
      events: [{
        subject: "local-cli",
        timestampMs: "123",
        nodeId: "node-a",
        capability: "fs:workspace",
        action: "stat",
        resource: "/a.txt",
        allowed: true,
        reason: "allowed",
      }],
      nextPageToken: "",
    });
    niceGrpcMock.client.explainCapability.mockResolvedValueOnce({
      subject: "local-cli",
      capabilityId: "fs:workspace",
      action: "read",
      resource: "/a.txt",
      allowed: true,
      reasonCode: "allowed",
      message: "allowed",
    });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);

    await expect(client.listCapabilities("node-a")).resolves.toEqual({
      capabilities: [{
        id: "fs:workspace",
        kind: "fs",
        node_id: "node-a",
        name: "workspace",
        permissions: ["read"],
        description: "workspace fs",
      }],
    });
    await expect(client.statFs("node-a", "/a.txt")).resolves.toMatchObject({ path: "/a.txt", size: 3 });
    await expect(client.listFs("node-a", "/")).resolves.toMatchObject({ path: "/", entries: [] });
    expect(niceGrpcMock.client.listFs).toHaveBeenCalledWith(
      { path: "/", pageSize: 1000, pageToken: "" },
      expect.any(Object),
    );
    await expect(client.runExec("node-a", { command: "true", timeoutSecs: 5 })).resolves.toMatchObject({ id: "exec-1", status: "running" });
    expect(niceGrpcMock.client.runExec).toHaveBeenCalledWith(
      expect.objectContaining({ command: "true", argv: [], timeoutSecs: "5" }),
      expect.any(Object),
    );
    await expect(client.getExec("node-a", "exec-1")).resolves.toMatchObject({ id: "exec-1", status: "succeeded" });
    await expect(client.cancelExec("node-a", "exec-1")).resolves.toMatchObject({ id: "exec-1", status: "cancelled" });
    await expect(client.listAudit("node-a")).resolves.toEqual({
      events: [{
        subject: "local-cli",
        timestamp_ms: 123,
        node_id: "node-a",
        capability: "fs:workspace",
        action: "stat",
        resource: "/a.txt",
        allowed: true,
        reason: "allowed",
        run_id: null,
        step_id: null,
      }],
    });
    await expect(client.explainCapability("node-a", {
      capability_id: "fs:workspace",
      action: "read",
      resource: "/a.txt",
    })).resolves.toEqual({
      subject: "local-cli",
      capability_id: "fs:workspace",
      action: "read",
      resource: "/a.txt",
      allowed: true,
      reason_code: "allowed",
      message: "allowed",
    });
    expect(niceGrpcMock.client.explainCapability).toHaveBeenCalledWith(
      { capabilityId: "fs:workspace", action: "read", resource: "/a.txt", timeoutSecs: undefined },
      expect.any(Object),
    );
  });

  it("sends argv exec requests without shell command text", async () => {
    niceGrpcMock.client.runExec.mockResolvedValue({
      id: "exec-argv",
      nodeId: "node-a",
      command: "printf hello world",
      cwd: "/",
      status: ExecStatus.EXEC_STATUS_RUNNING,
      logCount: "0",
      logsTruncated: false,
    });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    await client.runExec("node-a", { argv: ["printf", "hello world"], timeoutSecs: 5 });

    expect(niceGrpcMock.client.runExec).toHaveBeenCalledWith(
      expect.objectContaining({
        command: "",
        argv: ["printf", "hello world"],
        timeoutSecs: "5",
      }),
      expect.any(Object),
    );
  });

  it("copies files through daemon-side fs copy", async () => {
    niceGrpcMock.client.copyFs.mockResolvedValue({
      fromPath: "/a.txt",
      toPath: "/b.txt",
      bytesCopied: "7",
      version: "v1:file:7:123",
    });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const result = await client.copyFile("node-a", "/a.txt", "/b.txt");

    expect(result).toEqual({ from_path: "/a.txt", to_path: "/b.txt", bytes_copied: 7, version: "v1:file:7:123" });
    expect(niceGrpcMock.client.copyFs).toHaveBeenCalledWith(
      { fromPath: "/a.txt", toPath: "/b.txt" },
      expect.any(Object),
    );
  });

  it("reads byte ranges through the range-read protocol", async () => {
    const data = new Uint8Array([0x42, 0x43]);
    niceGrpcMock.client.readFileRange.mockResolvedValue({ data });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const result = await client.readFileRangeBytes("node-a", "/large.bin", 1024, 2);

    expect(result).toEqual(data);
    expect(niceGrpcMock.client.readFileRange).toHaveBeenCalledWith(
      { path: "/large.bin", offset: "1024", size: 2 },
      expect.any(Object),
    );
  });

  it("passes readable stream bodies to writeFile without pre-reading them", async () => {
    niceGrpcMock.client.writeFile.mockImplementation(async (requests: AsyncIterable<unknown>) => {
      const iterator = requests[Symbol.asyncIterator]();
      await expect(iterator.next()).resolves.toEqual({
        done: false,
        value: {
          target: {
            path: "/stream.bin",
            precondition: undefined,
            expectedVersion: undefined,
            requireAbsent: false,
          },
        },
      });
      return { path: "/stream.bin", bytesWritten: "0" };
    });
    let pulls = 0;
    const body = new ReadableStream<Uint8Array>({
      pull(controller) {
        pulls += 1;
        if (pulls <= 3) {
          controller.enqueue(new Uint8Array([pulls]));
        } else {
          controller.close();
        }
      },
    });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    await client.writeFileBytes("node-a", "/stream.bin", body);

    expect(pulls).toBeLessThan(3);
  });

  it("passes filesystem write expected versions as gRPC preconditions", async () => {
    niceGrpcMock.client.writeFile.mockImplementation(async (requests: AsyncIterable<unknown>) => {
      const iterator = requests[Symbol.asyncIterator]();
      await expect(iterator.next()).resolves.toEqual({
        done: false,
        value: {
          target: {
            path: "/guarded.txt",
            precondition: {
              expectedVersion: "v1:file:3:123",
              requireAbsent: false,
            },
            expectedVersion: "v1:file:3:123",
            requireAbsent: false,
          },
        },
      });
      return { path: "/guarded.txt", bytesWritten: "3", version: "v1:file:3:456" };
    });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    await expect(
      client.writeFileBytes("node-a", "/guarded.txt", "new", {
        expected_version: "v1:file:3:123",
      }),
    ).resolves.toEqual({
      path: "/guarded.txt",
      bytes_written: 3,
      version: "v1:file:3:456",
    });
  });

  it("opens service tunnels as binary streams", async () => {
    const response = new Uint8Array([0x48, 0x54, 0x54, 0x50]);
    niceGrpcMock.client.openServiceTunnel.mockReturnValue(asyncIterable([
      { opened: { serviceId: "web", host: "127.0.0.1", port: 80 } },
      { data: { data: response } },
      { close: { reason: "remote service closed" } },
    ]));

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789", token: "test-token" }]);
    const stream = await client.openServiceTunnel("node-a", "web", asyncIterable([new TextEncoder().encode("GET / HTTP/1.0\r\n\r\n")]));
    const reader = stream.getReader();

    await expect(reader.read()).resolves.toEqual({ done: false, value: response });
    await expect(reader.read()).resolves.toEqual({ done: true, value: undefined });
    expect(niceGrpcMock.client.openServiceTunnel).toHaveBeenCalledWith(expect.any(Object), expect.any(Object));
    const requestStream = niceGrpcMock.client.openServiceTunnel.mock.calls[0][0] as AsyncIterable<unknown>;
    const requests = [];
    for await (const request of requestStream) {
      requests.push(request);
    }
    expect(requests[0]).toEqual({ target: { serviceId: "web" } });
    expect(requests[1]).toEqual({ data: { data: new TextEncoder().encode("GET / HTTP/1.0\r\n\r\n") } });
    expect(requests[2]).toEqual({ close: { reason: "client input ended" } });
  });

  it("opens UDP service datagram tunnels with peer ids", async () => {
    const requestData = new Uint8Array([0x01, 0x02]);
    const responseData = new Uint8Array([0x03, 0x04]);
    niceGrpcMock.client.openServiceDatagramTunnel.mockReturnValue(asyncIterable([
      { opened: { serviceId: "dns", host: "127.0.0.1", port: 5353 } },
      { datagram: { peerId: "peer-1", data: responseData } },
      { close: { peerId: "peer-1", reason: "peer session idle timeout" } },
    ]));

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789", token: "test-token" }]);
    const events = [];
    for await (const event of await client.openServiceDatagramTunnel("node-a", "dns", asyncIterable([{ peer_id: "peer-1", data: requestData }]))) {
      events.push(event);
    }

    expect(events).toEqual([
      { type: "opened", service_id: "dns", host: "127.0.0.1", port: 5353 },
      { type: "datagram", peer_id: "peer-1", data: responseData },
      { type: "close", peer_id: "peer-1", reason: "peer session idle timeout" },
    ]);
    expect(niceGrpcMock.client.openServiceDatagramTunnel).toHaveBeenCalledWith(expect.any(Object), expect.any(Object));
    const requestStream = niceGrpcMock.client.openServiceDatagramTunnel.mock.calls[0][0] as AsyncIterable<unknown>;
    const requests = [];
    for await (const request of requestStream) {
      requests.push(request);
    }
    expect(requests[0]).toEqual({ target: { serviceId: "dns" } });
    expect(requests[1]).toEqual({ datagram: { peerId: "peer-1", data: requestData } });
    expect(requests[2]).toEqual({ close: { peerId: "", reason: "client input ended" } });
  });

  it("streams exec logs as bytes without string re-encoding", async () => {
    const first = new Uint8Array([0xff, 0x00, 0x41]);
    const second = new Uint8Array([0x42]);
    niceGrpcMock.client.streamExecLogs.mockReturnValue(asyncIterable([
      {
        snapshot: {
          execId: "exec-1",
          logs: [{ stream: "stdout", data: first, sequence: "0" }],
          truncated: false,
          droppedLogCount: "0",
          nextSequence: "1",
        },
      },
      { entry: { execId: "exec-1", log: { stream: "stderr", data: second, sequence: "1" } } },
      {
        complete: {
          execId: "exec-1",
          status: ExecStatus.EXEC_STATUS_SUCCEEDED,
          exitCode: 0,
          logCount: "2",
          logsTruncated: false,
          truncated: false,
          droppedLogCount: "0",
        },
      },
    ]));

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const reader = (await client.streamExecLogs("node-a", "exec-1")).getReader();

    await expect(reader.read()).resolves.toEqual({ done: false, value: first });
    await expect(reader.read()).resolves.toEqual({ done: false, value: second });
    await expect(reader.read()).resolves.toEqual({ done: true, value: undefined });
  });

  it("exposes typed exec log stream envelope events", async () => {
    const data = new Uint8Array([0x41]);
    niceGrpcMock.client.streamExecLogs.mockReturnValue(asyncIterable([
      {
        snapshot: {
          execId: "exec-1",
          logs: [{ stream: "stdout", data, sequence: "3" }],
          truncated: true,
          droppedLogCount: "3",
          nextSequence: "4",
        },
      },
    ]));

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const events = [];
    for await (const event of await client.streamExecLogEvents("node-a", "exec-1")) {
      events.push(event);
    }

    expect(events).toEqual([
      {
        type: "snapshot",
        snapshot: {
          exec_id: "exec-1",
          logs: [{ stream: "stdout", data, sequence: 3 }],
          truncated: true,
          dropped_log_count: 3,
          next_sequence: 4,
        },
      },
    ]);
  });

  it("runs fs.copy steps over gRPC", async () => {
    niceGrpcMock.client.copyFs.mockResolvedValue({
      fromPath: "/a.txt",
      toPath: "/b.txt",
      bytesCopied: "7",
      version: "v1:file:7:123",
    });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const trace = await client.run({
      name: "copy",
      steps: [{ id: "copy", node: "node-a", action: "fs.copy", fromPath: "/a.txt", toPath: "/b.txt" }],
    });

    expect(trace.status).toBe("succeeded");
    expect(trace.steps[0].output).toEqual({ from_path: "/a.txt", to_path: "/b.txt", bytes_copied: 7, version: "v1:file:7:123" });
  });

  it("opens exec session streams with start metadata and input chunks", async () => {
    niceGrpcMock.client.openExecSession.mockReturnValue(asyncIterable([
      { started: { execId: "exec-1" } },
      { output: { execId: "exec-1", data: new TextEncoder().encode("hello") } },
      { exit: { execId: "exec-1", status: ExecStatus.EXEC_STATUS_SUCCEEDED, exitCode: 0 } },
    ]));

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const events: unknown[] = [];
    const stream = await client.openExecSession(
      "node-a",
      { argv: ["/bin/sh"], cwd: "/", rows: 30, cols: 100 },
      asyncIterable([new TextEncoder().encode("exit\n")]),
    );
    for await (const event of stream) {
      events.push(event);
    }

    expect(events).toEqual([
      { type: "started", exec_id: "exec-1" },
      { type: "output", exec_id: "exec-1", data: new TextEncoder().encode("hello") },
      { type: "exit", exec_id: "exec-1", status: "succeeded", exit_code: 0 },
    ]);
    const requests = niceGrpcMock.client.openExecSession.mock.calls[0][0];
    const sent = [];
    for await (const request of requests) {
      sent.push(request);
    }
    expect(sent[0]).toMatchObject({ start: { argv: ["/bin/sh"], cwd: "/", rows: 30, cols: 100 } });
    expect(sent[1]).toMatchObject({ input: { data: new TextEncoder().encode("exit\n") } });
  });
});

async function* asyncIterable<T>(items: T[]): AsyncIterable<T> {
  for (const item of items) {
    yield item;
  }
}
