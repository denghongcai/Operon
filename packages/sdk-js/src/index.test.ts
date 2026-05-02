import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { JobStatus, ServiceProtocol } from "./generated/operon/runtime";
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
      readFile: vi.fn(),
      writeFile: vi.fn(),
      copyFs: vi.fn(),
      runJob: vi.fn(),
      getJob: vi.fn(),
      listJobs: vi.fn(),
      watchJob: vi.fn(),
      listJobLogs: vi.fn(),
      streamJobLogs: vi.fn(),
      writeJobStdin: vi.fn(),
      closeJobStdin: vi.fn(),
      listServices: vi.fn(),
      checkService: vi.fn(),
      openServiceTunnel: vi.fn(),
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
  it("runs fs and job steps sequentially over gRPC and returns a successful trace", async () => {
    niceGrpcMock.client.writeFile.mockResolvedValue({ path: "/input.txt", bytesWritten: 5 });
    niceGrpcMock.client.runJob.mockResolvedValue({
      id: "job-1",
      nodeId: "node-a",
      command: "cat input.txt",
      cwd: "/",
      status: JobStatus.JOB_STATUS_RUNNING,
      logCount: "0",
      logsTruncated: false,
    });
    niceGrpcMock.client.watchJob.mockReturnValue(asyncIterable([
      {
        jobId: "job-1",
        status: JobStatus.JOB_STATUS_RUNNING,
        logCount: "0",
        logsTruncated: false,
      },
      {
        jobId: "job-1",
        status: JobStatus.JOB_STATUS_SUCCEEDED,
        exitCode: 0,
        logCount: "1",
        logsTruncated: false,
      },
    ]));
    niceGrpcMock.client.getJob.mockResolvedValueOnce({
      id: "job-1",
      nodeId: "node-a",
      command: "cat input.txt",
      cwd: "/",
      status: JobStatus.JOB_STATUS_SUCCEEDED,
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
        { id: "run", node: "node-a", action: "job.run", command: "cat input.txt", secrets: ["GITHUB_TOKEN"] },
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
    expect(niceGrpcMock.client.runJob).toHaveBeenCalledWith(
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
          protocol: ServiceProtocol.SERVICE_PROTOCOL_TCP,
          description: "application",
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
    expect(services.services[0].port).toBe(7789);
    expect(check.ok).toBe(true);
    expect(niceGrpcMock.client.listServices).toHaveBeenCalledWith({ pageSize: 1000, pageToken: "" }, expect.any(Object));
    expect(niceGrpcMock.client.listServices).toHaveBeenCalledWith({ pageSize: 1000, pageToken: "1" }, expect.any(Object));
    expect(niceGrpcMock.client.checkService).toHaveBeenCalledWith({ serviceId: "daemon" }, expect.any(Object));
  });

  it("copies files through daemon-side fs copy", async () => {
    niceGrpcMock.client.copyFs.mockResolvedValue({
      fromPath: "/a.txt",
      toPath: "/b.txt",
      bytesCopied: "7",
    });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const result = await client.copyFile("node-a", "/a.txt", "/b.txt");

    expect(result).toEqual({ from_path: "/a.txt", to_path: "/b.txt", bytes_copied: 7 });
    expect(niceGrpcMock.client.copyFs).toHaveBeenCalledWith(
      { fromPath: "/a.txt", toPath: "/b.txt" },
      expect.any(Object),
    );
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

  it("streams job logs as bytes without string re-encoding", async () => {
    const first = new Uint8Array([0xff, 0x00, 0x41]);
    const second = new Uint8Array([0x42]);
    niceGrpcMock.client.streamJobLogs.mockReturnValue(asyncIterable([
      {
        snapshot: {
          jobId: "job-1",
          logs: [{ stream: "stdout", data: first, sequence: "0" }],
          truncated: false,
          droppedLogCount: "0",
          nextSequence: "1",
        },
      },
      { entry: { jobId: "job-1", log: { stream: "stderr", data: second, sequence: "1" } } },
      {
        complete: {
          jobId: "job-1",
          status: JobStatus.JOB_STATUS_SUCCEEDED,
          exitCode: 0,
          logCount: "2",
          logsTruncated: false,
          truncated: false,
          droppedLogCount: "0",
        },
      },
    ]));

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const reader = (await client.streamJobLogs("node-a", "job-1")).getReader();

    await expect(reader.read()).resolves.toEqual({ done: false, value: first });
    await expect(reader.read()).resolves.toEqual({ done: false, value: second });
    await expect(reader.read()).resolves.toEqual({ done: true, value: undefined });
  });

  it("exposes typed job log stream envelope events", async () => {
    const data = new Uint8Array([0x41]);
    niceGrpcMock.client.streamJobLogs.mockReturnValue(asyncIterable([
      {
        snapshot: {
          jobId: "job-1",
          logs: [{ stream: "stdout", data, sequence: "3" }],
          truncated: true,
          droppedLogCount: "3",
          nextSequence: "4",
        },
      },
    ]));

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const events = [];
    for await (const event of await client.streamJobLogEvents("node-a", "job-1")) {
      events.push(event);
    }

    expect(events).toEqual([
      {
        type: "snapshot",
        snapshot: {
          job_id: "job-1",
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
    });

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "grpc://127.0.0.1:7789" }]);
    const trace = await client.run({
      name: "copy",
      steps: [{ id: "copy", node: "node-a", action: "fs.copy", fromPath: "/a.txt", toPath: "/b.txt" }],
    });

    expect(trace.status).toBe("succeeded");
    expect(trace.steps[0].output).toEqual({ from_path: "/a.txt", to_path: "/b.txt", bytes_copied: 7 });
  });
});

async function* asyncIterable<T>(items: T[]): AsyncIterable<T> {
  for (const item of items) {
    yield item;
  }
}
