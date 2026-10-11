import { describe, expect, it, vi } from "vitest";
import { mapGrpcServiceDatagramTunnelEvents } from "./grpc-mappers";
import type { ServiceDatagramTunnelResponse } from "./generated/operon/runtime";

describe("stream mapper cancellation", () => {
  it("forwards return before waiting on a pending source next and closes once", async () => {
    let rejectNext: (error: Error) => void = () => undefined;
    const close = vi.fn(async () => { rejectNext(new Error("cancelled")); return { done: true as const, value: undefined }; });
    const source = {
      [Symbol.asyncIterator]() { return {
        next() { return new Promise<IteratorResult<ServiceDatagramTunnelResponse>>((_, reject) => { rejectNext = reject; }); },
        return: close,
      }; },
    };
    const iterator = mapGrpcServiceDatagramTunnelEvents(source)[Symbol.asyncIterator]();
    const pending = expect(iterator.next()).rejects.toThrow("cancelled");
    await expect(iterator.return?.()).resolves.toMatchObject({ done: true });
    await pending;
    expect(close).toHaveBeenCalledOnce();
  });
  it("keeps cleanup failures observable", async () => {
    const source = { [Symbol.asyncIterator]() { return {
      async next() { return { done: true as const, value: undefined }; },
      async return() { throw new Error("cleanup failed"); },
    }; } };
    const iterator = mapGrpcServiceDatagramTunnelEvents(source)[Symbol.asyncIterator]();
    await expect(iterator.return?.()).rejects.toThrow("cleanup failed");
  });
});
