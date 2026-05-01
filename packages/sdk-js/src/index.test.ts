import { afterEach, describe, expect, it, vi } from "vitest";
import { OperonClient } from "./index";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("OperonClient", () => {
  it("runs fs and job steps sequentially and returns a successful trace", async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(jsonResponse({ path: "/input.txt", bytes_written: 5 }))
      .mockResolvedValueOnce(jsonResponse({ id: "job-1", status: "running" }))
      .mockResolvedValueOnce(jsonResponse({ id: "job-1", status: "running" }))
      .mockResolvedValueOnce(
        jsonResponse({
          id: "job-1",
          node_id: "node-a",
          command: "cat input.txt",
          cwd: "/",
          status: "succeeded",
          exit_code: 0,
          logs: [{ stream: "stdout", data: "hello" }],
        }),
      )
      .mockResolvedValueOnce(jsonResponse({ path: "/output.txt", content: "hello" }));
    vi.stubGlobal("fetch", fetchMock);

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "http://127.0.0.1:7788", token: "test-token" }]);
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
    expect(fetchMock).toHaveBeenCalledTimes(5);
    expect(String(fetchMock.mock.calls[0][0])).toBe("http://127.0.0.1:7788/fs/write");
    expect(String(fetchMock.mock.calls[2][0])).toBe("http://127.0.0.1:7788/job/status?id=job-1");
    expect((fetchMock.mock.calls[0][1]?.headers as Headers).get("authorization")).toBe("Bearer test-token");
    expect(JSON.parse(String(fetchMock.mock.calls[1][1]?.body)).secrets).toEqual(["GITHUB_TOKEN"]);
  });

  it("stops on the first failed step and returns a failed trace", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn<typeof fetch>()
        .mockResolvedValueOnce(jsonResponse({ code: "forbidden", message: "fs read denied by policy" }, 403, "Forbidden")),
    );

    const client = new OperonClient([{ nodeId: "node-a", endpoint: "http://127.0.0.1:7788" }]);
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
});

function jsonResponse(body: unknown, status = 200, statusText = "OK"): Response {
  const text = JSON.stringify(body);
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText,
    headers: new Headers({ "content-type": "application/json" }),
    json: async () => body,
    text: async () => text,
    arrayBuffer: async () => new TextEncoder().encode(text).buffer,
  } as Response;
}
