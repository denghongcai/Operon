import type { ExecSessionRequest as GrpcExecSessionRequest } from "./generated/operon/runtime";
import type { ExecSessionStart, FsPrecondition, ServiceDatagram } from "./types";

import { bodyToByteChunks } from "./transport";

export async function* grpcFileChunks(
  path: string,
  bytes: Uint8Array,
  precondition?: FsPrecondition,
): AsyncIterable<{
  target?: ReturnType<typeof grpcFileTarget>;
  chunk?: { data: Uint8Array };
}> {
  yield { target: grpcFileTarget(path, precondition) };
  if (bytes.byteLength === 0) {
    yield { chunk: { data: new Uint8Array() } };
    return;
  }
  for (let offset = 0; offset < bytes.byteLength; offset += 64 * 1024) {
    yield {
      chunk: { data: bytes.subarray(offset, Math.min(offset + 64 * 1024, bytes.byteLength)) },
    };
  }
}

export async function* grpcFileChunksFromBody(
  path: string,
  body: BodyInit,
  precondition?: FsPrecondition,
): AsyncIterable<{
  target?: ReturnType<typeof grpcFileTarget>;
  chunk?: { data: Uint8Array };
}> {
  yield { target: grpcFileTarget(path, precondition) };
  let emitted = false;
  for await (const bytes of bodyToByteChunks(body)) {
    if (bytes.byteLength === 0) {
      continue;
    }
    emitted = true;
    for (let offset = 0; offset < bytes.byteLength; offset += 64 * 1024) {
      yield {
        chunk: { data: bytes.subarray(offset, Math.min(offset + 64 * 1024, bytes.byteLength)) },
      };
    }
  }
  if (!emitted) {
    yield { chunk: { data: new Uint8Array() } };
  }
}

export function grpcFileTarget(path: string, precondition?: FsPrecondition) {
  const requireAbsent = precondition?.require_absent ?? false;
  const expectedVersion = precondition?.expected_version;
  return {
    path,
    precondition:
      expectedVersion || requireAbsent
        ? { expectedVersion, requireAbsent }
        : undefined,
    expectedVersion,
    requireAbsent,
  };
}

export async function* grpcStdinChunks(
  execId: string,
  bytes: Uint8Array,
): AsyncIterable<{ target?: { execId: string }; chunk?: { data: Uint8Array } }> {
  yield { target: { execId } };
  if (bytes.byteLength === 0) {
    yield { chunk: { data: new Uint8Array() } };
    return;
  }
  for (let offset = 0; offset < bytes.byteLength; offset += 64 * 1024) {
    yield {
      chunk: { data: bytes.subarray(offset, Math.min(offset + 64 * 1024, bytes.byteLength)) },
    };
  }
}

export async function* grpcExecSessionRequests(
  start: ExecSessionStart,
  input: AsyncIterable<Uint8Array>,
): AsyncIterable<GrpcExecSessionRequest> {
  yield {
    start: {
      command: start.command ?? "",
      argv: start.argv ?? [],
      cwd: start.cwd ?? "",
      timeoutSecs: start.timeoutSecs === undefined ? undefined : String(start.timeoutSecs),
      secrets: start.secrets ?? [],
      rows: start.rows ?? 24,
      cols: start.cols ?? 80,
    },
  };
  for await (const chunk of input) {
    yield { input: { data: chunk } };
  }
}

export async function* emptyAsyncIterable(): AsyncIterable<Uint8Array> {}

export async function* grpcServiceTunnelRequests(
  serviceId: string,
  input: AsyncIterable<Uint8Array>,
): AsyncIterable<{ target?: { serviceId: string }; data?: { data: Uint8Array }; close?: { reason: string } }> {
  yield { target: { serviceId } };
  for await (const chunk of input) {
    yield { data: { data: chunk } };
  }
  yield { close: { reason: "client input ended" } };
}

export async function* grpcServiceDatagramTunnelRequests(
  serviceId: string,
  input: AsyncIterable<ServiceDatagram>,
): AsyncIterable<{
  target?: { serviceId: string };
  datagram?: { peerId: string; data: Uint8Array };
  close?: { peerId: string; reason: string };
}> {
  yield { target: { serviceId } };
  for await (const datagram of input) {
    yield { datagram: { peerId: datagram.peer_id, data: datagram.data } };
  }
  yield { close: { peerId: "", reason: "client input ended" } };
}
