import { afterEach, describe, expect, it, vi } from "vitest";
import { Metadata, Status } from "nice-grpc";
import { embeddedClient } from "./embedded-client";
import { GrpcClientPool } from "./client-transport";
import { HealthStatus } from "./generated/operon/runtime";
import sdkPackage from "../package.json";

afterEach(() => vi.restoreAllMocks());

const endpoint = { nodeId: "logical", endpoint: `iroh://${"ab".repeat(32)}`, token: "test-token",
  transport: { iroh: { identityFile: "/private/client.key", peerAddrs: ["127.0.0.1:7788"] } } };

function fakeBinding(status?: Error) {
  const close = vi.fn();
  const configs: unknown[] = [];
  const messages: Buffer[] = [];
  vi.spyOn(process, "dlopen").mockImplementation(module => {
    module.exports = { bindingVersion: () => sdkPackage.version, NativeClient: class {
      constructor(config: string) { configs.push(JSON.parse(config)); }
      async ready() {}
      close = close;
      async openCall() {
        let delivered = false;
        return {
          async send(data: Buffer) { messages.push(data); },
          async finishSend() {},
          async headers() { if (status) throw status; return '[["x-test","header",false]]'; },
          trailers() { return '[["x-test-bin","AQID",true]]'; },
          async receive() {
            if (delivered) return null;
            delivered = true;
            return Buffer.from(HealthStatus.encode(HealthStatus.fromPartial({ ok: true, version: "test" })).finish());
          },
          cancel() {},
          async close() {},
        };
      }
    } };
  });
  return { close, configs, messages };
}

describe("optional embedded SDK adapter", () => {
  it("default clients do not load native code", () => {
    const load = vi.spyOn(process, "dlopen");
    const pool = new GrpcClientPool([{ nodeId: "local", endpoint: "grpc://127.0.0.1:7789" }]);
    pool.client(pool.endpoint("local"));
    pool.close();
    expect(load).not.toHaveBeenCalled();
  });
  it("rejects backend/config mismatch without fallback", () => {
    expect(() => embeddedClient({ nodeId: "x", endpoint: endpoint.endpoint })).toThrow("transport.iroh");
    const pool = new GrpcClientPool([{ ...endpoint, endpoint: "grpc://127.0.0.1:7789" }]);
    expect(() => pool.client(pool.endpoint("logical"))).toThrow("requires an iroh endpoint");
  });
  it("encodes protobuf and carries header/binary trailer callbacks with separate logical identity", async () => {
    const fake = fakeBinding();
    const entry = embeddedClient(endpoint);
    const header = vi.fn();
    const trailer = vi.fn();
    await expect(entry.client.health({}, { metadata: Metadata().set("x-test", "request"), onHeader: header, onTrailer: trailer })).resolves.toMatchObject({ ok: true });
    expect(fake.configs[0]).toMatchObject({ node_id: "logical", endpoint: endpoint.endpoint, transport: { iroh: { identity_file: "/private/client.key" } } });
    expect(fake.messages).toHaveLength(1);
    expect(header.mock.calls[0][0].get("x-test")).toBe("header");
    expect([...trailer.mock.calls[0][0].get("x-test-bin")]).toEqual([1, 2, 3]);
    entry.close();
    expect(fake.close).toHaveBeenCalledOnce();
  });
  it("preserves gRPC status instead of returning a successful empty response", async () => {
    fakeBinding(new Error("grpc-status:16:invalid bearer token"));
    const entry = embeddedClient(endpoint);
    await expect(entry.client.health({})).rejects.toMatchObject({ code: Status.UNAUTHENTICATED });
    entry.close();
  });
  it("rejects mismatched or legacy bindings rather than falling back", () => {
    vi.spyOn(process, "dlopen").mockImplementation(module => { module.exports = { NativeClient: class {}, bindingVersion: () => "0.0.0" }; });
    expect(() => embeddedClient(endpoint)).toThrow("must match SDK version");
  });
  it("iterator return interrupts a pending native read before joining teardown", async () => {
    const closed = vi.fn();
    let rejectRead: ((error: Error) => void) | undefined;
    const reading = new Promise<void>(resolve => {
      vi.spyOn(process, "dlopen").mockImplementation(module => {
        module.exports = { bindingVersion: () => sdkPackage.version, NativeClient: class {
          async ready() {}
          close() {}
          async openCall() { return {
            async send() {}, async finishSend() {}, async headers() { return "[]"; }, trailers() { return "[]"; },
            receive() { return new Promise<Buffer>((_, reject) => { rejectRead = reject; resolve(); }); },
            cancel() { rejectRead?.(new Error("call cancelled")); }, async close() { closed(); },
          }; }
        } };
      });
    });
    const entry = embeddedClient({ ...endpoint, transport: { ...endpoint.transport, rpcTimeoutSecs: 0, progressTimeoutSecs: 0 } });
    const iterator = entry.client.readFile({ path: "/pending" });
    const pending = expect(iterator.next()).rejects.toMatchObject({ code: Status.CANCELLED });
    await reading;
    const returned = iterator.return();
    await pending;
    await expect(returned).resolves.toMatchObject({ done: true });
    expect(closed).toHaveBeenCalledOnce();
    entry.close();
  });
});
