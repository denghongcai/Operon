import { Metadata, type CallOptions } from "nice-grpc";

import type { NodeEndpoint } from "./index";

export const DEFAULT_LIST_PAGE_SIZE = 1000;

export type RequestContext = {
  runId?: string;
  stepId?: string;
};

export function required(value: string | undefined, field: string): string {
  if (!value) {
    throw new Error(`step requires ${field}`);
  }
  return value;
}

export function grpcTarget(endpoint: string): string {
  if (endpoint.startsWith("grpc://")) {
    return `http://${endpoint.slice("grpc://".length)}`;
  }
  if (endpoint.startsWith("grpcs://")) {
    return `https://${endpoint.slice("grpcs://".length)}`;
  }
  throw new Error(`endpoint ${endpoint} is not a gRPC endpoint`);
}

export function grpcOptions(endpoint: NodeEndpoint, context?: RequestContext): CallOptions {
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
  return { metadata };
}

export async function bodyToBytes(body: BodyInit): Promise<Uint8Array> {
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
    return streamToBytes(body);
  }
  throw new Error("unsupported BodyInit for gRPC streaming request");
}

export async function* bodyToByteChunks(body: BodyInit): AsyncIterable<Uint8Array> {
  if (body instanceof ReadableStream) {
    const reader = body.getReader();
    while (true) {
      const next = await reader.read();
      if (next.done) {
        return;
      }
      yield next.value;
    }
  } else {
    yield await bodyToBytes(body);
  }
}

export function concatChunks(chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0);
  const merged = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return merged;
}

export function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const buffer = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(buffer).set(bytes);
  return buffer;
}

export async function streamToBytes(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
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
