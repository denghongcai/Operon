import { afterEach, describe, expect, it, vi } from "vitest";
import { Metadata, Status, type ClientMiddlewareCall } from "nice-grpc";
import { deadlineMiddleware, transportConfig } from "./transport-deadlines";

afterEach(() => vi.useRealTimers());

function pendingCall(method: string): ClientMiddlewareCall<{}, {}> {
  return {
    method: { path: `/operon.runtime.v1.OperonRuntime/${method}`, requestStream: false, responseStream: false, options: {} },
    requestStream: false, responseStream: false, request: {},
    async *next(_request, options) {
      options.onHeader?.(Metadata());
      await new Promise((_, reject) => {
        const abort = () => reject(new Error("aborted"));
        if (options.signal?.aborted) abort();
        else options.signal?.addEventListener("abort", abort, { once: true });
      });
      return {};
    },
  };
}

describe("transport deadlines", () => {
  it("encodes the largest configured deadline within the gRPC eight-digit limit", async () => {
    const call = pendingCall("StatFs");
    call.next = async function* (_request, options) {
      expect(options.metadata?.get("grpc-timeout")).toBe("604800S");
      return {};
    };
    const iterator = deadlineMiddleware(transportConfig({ rpcTimeoutSecs: 604800 }))(call, {});
    expect((await iterator.next()).done).toBe(true);
  });
  it("preserves graceful early stream return without manufacturing cancellation", async () => {
    const call = pendingCall("WatchExec");
    let cleaned = false;
    call.responseStream = true;
    call.next = async function* (_request, options) {
      options.onHeader?.(Metadata());
      try {
        yield {};
        yield {};
      } finally {
        cleaned = true;
        if (options.signal?.aborted) throw new Error("The operation has been aborted");
      }
    };
    const iterator = deadlineMiddleware(transportConfig())(call, {});
    expect((await iterator.next()).done).toBe(false);
    await expect(iterator.return(undefined)).resolves.toMatchObject({ done: true });
    expect(cleaned).toBe(true);
  });
  it("validates configurable values and permits explicit disable", () => {
    expect(transportConfig({ rpcTimeoutSecs: 300, progressTimeoutSecs: 0 })).toMatchObject({ rpcTimeoutSecs: 300, progressTimeoutSecs: 0, connectTimeoutSecs: 10 });
    for (const value of [-1, Infinity, NaN, 604801]) {
      expect(() => transportConfig({ rpcTimeoutSecs: value })).toThrow();
    }
  });

  it("expires a non-responsive ordinary call and preserves auth metadata", async () => {
    vi.useFakeTimers();
    const iterator = deadlineMiddleware(transportConfig({ rpcTimeoutSecs: 1 }))(pendingCall("StatFs"), { metadata: Metadata().set("authorization", "Bearer test") });
    const result = expect(iterator.next()).rejects.toMatchObject({ code: Status.DEADLINE_EXCEEDED });
    await vi.advanceTimersByTimeAsync(1001);
    await result;
    expect(vi.getTimerCount()).toBe(0);
  });

  it("does not attach a short lifetime deadline to idle tunnels", async () => {
    vi.useFakeTimers();
    const controller = new AbortController();
    const iterator = deadlineMiddleware(transportConfig({ rpcTimeoutSecs: 1 }))(pendingCall("OpenServiceTunnel"), { signal: controller.signal });
    const result = expect(iterator.next()).rejects.toThrow("aborted");
    await vi.advanceTimersByTimeAsync(30000);
    expect(vi.getTimerCount()).toBe(0);
    controller.abort();
    await result;
  });

  it("bounds long-lived stream establishment but not its healthy idle lifetime", async () => {
    vi.useFakeTimers();
    const call = pendingCall("OpenServiceTunnel");
    call.next = async function* (_request, options) {
      await new Promise((_, reject) => options.signal?.addEventListener("abort", () => reject(new Error("aborted")), { once: true }));
      return {};
    };
    const iterator = deadlineMiddleware(transportConfig({ rpcTimeoutSecs: 1 }))(call, {});
    const result = expect(iterator.next()).rejects.toMatchObject({ code: Status.DEADLINE_EXCEEDED });
    await vi.advanceTimersByTimeAsync(1001);
    await result;
    expect(vi.getTimerCount()).toBe(0);
  });

  it("cancels while waiting for connection readiness", async () => {
    const controller = new AbortController();
    const iterator = deadlineMiddleware(transportConfig(), () => new Promise(() => {}))(pendingCall("StatFs"), { signal: controller.signal });
    const result = expect(iterator.next()).rejects.toMatchObject({ code: Status.CANCELLED });
    controller.abort();
    await result;
  });
});
