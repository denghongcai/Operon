import path from "node:path";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { ClientError, Metadata, Status, type CallOptions, type ClientMiddlewareCall } from "nice-grpc";
import { OperonRuntimeDefinition, type OperonRuntimeClient } from "./generated/operon/runtime";
import { deadlineMiddleware, transportConfig } from "./transport-deadlines";
import type { NodeEndpoint } from "./types";
import sdkPackage from "../package.json";
import { nativeTarget } from "./native-target";

interface NativeCall {
  send(data: Buffer): Promise<void>;
  finishSend(): Promise<void>;
  receive(): Promise<Buffer | null>;
  headers(): Promise<string>;
  trailers(): string;
  cancel(): void;
  close(): Promise<void>;
}
interface NativeClient {
  ready(): Promise<void>;
  openCall(path: string, metadata: string): Promise<NativeCall>;
  close(): void;
}
type Binding = { NativeClient: new (config: string) => NativeClient; bindingVersion(): string };
type Method = {
  name: string;
  requestStream: boolean;
  responseStream: boolean;
  options: Record<string, unknown>;
  requestType: { fromPartial(input: unknown): unknown; encode(input: unknown): { finish(): Uint8Array } };
  responseType: { decode(input: Uint8Array): unknown };
};

function loadBinding(): Binding {
  let filename = process.env.OPERON_SDK_NATIVE_PATH;
  if (filename === undefined) {
    const legacy = fileURLToPath(new URL("../native/operon-sdk-native.node", import.meta.url));
    if (existsSync(legacy)) filename = legacy;
    else {
      const report = process.platform === "linux" ? process.report.getReport() as { header?: { glibcVersionRuntime?: string } } : undefined;
      const target = nativeTarget(process.platform, process.arch, process.platform === "linux" && !report?.header?.glibcVersionRuntime);
      filename = fileURLToPath(new URL(`../native/${target}/operon-sdk-native.node`, import.meta.url));
    }
  }
  if (!path.isAbsolute(filename)) throw new Error("OPERON_SDK_NATIVE_PATH must be absolute");
  const binding = { exports: {} };
  try { process.dlopen(binding, filename); }
  catch (cause) { throw new Error("iroh requires the optional in-process Operon SDK native binding; no fallback was attempted", { cause }); }
  const result = binding.exports as Partial<Binding>;
  if (typeof result.NativeClient !== "function") throw new Error("invalid Operon SDK native binding");
  if (typeof result.bindingVersion !== "function" || result.bindingVersion() !== sdkPackage.version) {
    throw new Error(`native binding must match SDK version ${sdkPackage.version}; no fallback was attempted`);
  }
  return result as Binding;
}

function snakeCase(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(snakeCase);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([key, item]) => [key.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`), snakeCase(item)]));
  return value;
}

function decodeMetadata(json: string): Metadata {
  let result = Metadata();
  for (const [key, value, binary] of JSON.parse(json) as [string, string, boolean][]) {
    result = result.append(key, binary ? Buffer.from(value, "base64") : value);
  }
  return result;
}

function clientError(method: string, error: unknown): Error {
  if (error instanceof ClientError) return error;
  const message = error instanceof Error ? error.message : String(error);
  const status = /grpc-status:(\d+):([\s\S]*)/.exec(message);
  return new ClientError(method, status ? Number(status[1]) : message.includes("cancelled") || message.includes("closed") ? Status.CANCELLED : Status.UNAVAILABLE, status?.[2] ?? message);
}

async function abortable<T>(promise: Promise<T>, signal: AbortSignal): Promise<T> {
  if (signal.aborted) throw new Error("call cancelled");
  let abort: (() => void) | undefined;
  try {
    return await Promise.race([promise, new Promise<never>((_, reject) => {
      abort = () => reject(new Error("call cancelled"));
      signal.addEventListener("abort", abort, { once: true });
    })]);
  } finally { if (abort) signal.removeEventListener("abort", abort); }
}

export function embeddedClient(endpoint: NodeEndpoint): { client: OperonRuntimeClient; close(): void } {
  if (!endpoint.transport?.iroh) throw new Error("iroh endpoint requires transport.iroh; no fallback was attempted");
  const config = transportConfig(endpoint.transport);
  const native = new (loadBinding().NativeClient)(JSON.stringify(snakeCase({ ...endpoint, token: endpoint.token ?? null, transport: config })));
  const middleware = deadlineMiddleware(config, () => native.ready());
  const methods: Record<string, unknown> = {};
  for (const [name, definition] of Object.entries(OperonRuntimeDefinition.methods)) {
    const method = definition as unknown as Method;
    const methodPath = `/${OperonRuntimeDefinition.fullName}/${method.name}`;
    async function* exchange(request: unknown, options: CallOptions): AsyncGenerator<unknown, unknown, undefined> {
      const controller = new AbortController();
      const abort = () => controller.abort();
      if (options.signal?.aborted) abort();
      else options.signal?.addEventListener("abort", abort, { once: true });
      let call: NativeCall | undefined;
      let producer: Promise<void> | undefined;
      let producerFailure: unknown;
      const cancel = () => call?.cancel();
      controller.signal.addEventListener("abort", cancel, { once: true });
      try {
        const pairs: [string, string][] = [];
        for (const [key, values] of options.metadata ?? Metadata()) for (const value of values) {
          pairs.push([key, typeof value === "string" ? value : Buffer.from(value).toString("base64")]);
        }
        call = await native.openCall(methodPath, JSON.stringify(pairs));
        if (controller.signal.aborted) { call.cancel(); throw new Error("call cancelled"); }
        const activeCall = call;
        producer = (async () => {
          if (method.requestStream) {
            const iterator = (request as AsyncIterable<unknown>)[Symbol.asyncIterator]();
            try {
              while (true) {
                const next = await abortable(iterator.next(), controller.signal);
                if (next.done) break;
                await activeCall.send(Buffer.from(method.requestType.encode(method.requestType.fromPartial(next.value)).finish()));
              }
            } finally {
              // Arbitrary caller iterators may block their own return(). Do not
              // allow that to retain the native transport or pending RPC task.
              void Promise.resolve(iterator.return?.()).catch(() => undefined);
            }
          } else await activeCall.send(Buffer.from(method.requestType.encode(method.requestType.fromPartial(request)).finish()));
          await activeCall.finishSend();
        })().catch(error => { producerFailure = error; activeCall.cancel(); });
        options.onHeader?.(decodeMetadata(await call.headers()));
        let unary: unknown;
        let count = 0;
        while (true) {
          const message = await call.receive();
          if (message === null) break;
          const response = method.responseType.decode(message);
          if (method.responseStream) yield response;
          else { unary = response; if (++count > 1) throw new ClientError(methodPath, Status.INTERNAL, "unary RPC returned multiple responses"); }
        }
        options.onTrailer?.(decodeMetadata(call.trailers()));
        const uploadError = producerFailure;
        controller.abort();
        await producer;
        if (uploadError) throw uploadError;
        if (!method.responseStream && count !== 1) throw new ClientError(methodPath, Status.INTERNAL, "unary RPC returned no response");
        return unary;
      } catch (error) { throw clientError(methodPath, producerFailure ?? error); }
      finally {
        controller.abort();
        options.signal?.removeEventListener("abort", abort);
        if (call) await call.close();
        await producer;
      }
    }
    const invoke = (request: unknown, options: CallOptions = {}) => {
      const controller = new AbortController();
      const abort = () => controller.abort(options.signal?.reason);
      if (options.signal?.aborted) abort();
      else options.signal?.addEventListener("abort", abort, { once: true });
      const cleanup = () => options.signal?.removeEventListener("abort", abort);
      const iterator = middleware({
        method: { path: methodPath, requestStream: method.requestStream, responseStream: method.responseStream, options: method.options },
        request, requestStream: method.requestStream, responseStream: method.responseStream, next: exchange,
      } as unknown as ClientMiddlewareCall<unknown, unknown>, { ...options, signal: controller.signal });
      const next = iterator.next.bind(iterator);
      const finish = iterator.return.bind(iterator);
      const fail = iterator.throw.bind(iterator);
      iterator.next = async (...args) => {
        try { const result = await next(...args); if (result.done) cleanup(); return result; }
        catch (error) { cleanup(); throw error; }
      };
      // AsyncGenerator.return() alone cannot interrupt an awaited native read.
      // Abort the call first, then join generator and native-task teardown.
      iterator.return = async value => { controller.abort(); try { return await finish(value); } finally { cleanup(); } };
      iterator.throw = async error => { controller.abort(); try { return await fail(error); } finally { cleanup(); } };
      return iterator;
    };
    methods[name] = method.responseStream ? invoke : async (request: unknown, options: CallOptions = {}) => {
      const iterator = invoke(request, options);
      const result = await iterator.next();
      if (!result.done) { await iterator.return(undefined); throw new ClientError(methodPath, Status.INTERNAL, "unexpected unary middleware yield"); }
      return result.value;
    };
  }
  return { client: methods as unknown as OperonRuntimeClient, close: () => native.close() };
}
