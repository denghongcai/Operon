import { ClientError, Metadata, Status, type ClientMiddleware } from "nice-grpc";
import type { TransportConfig } from "./types";

export const DEFAULT_TRANSPORT: TransportConfig = {
  connectTimeoutSecs: 10,
  rpcTimeoutSecs: 30,
  transferTimeoutSecs: 600,
  progressTimeoutSecs: 60,
  keepaliveIntervalSecs: 30,
  keepaliveTimeoutSecs: 10,
  keepaliveWhileIdle: true,
};

export function transportConfig(overrides?: Partial<TransportConfig>): TransportConfig {
  const config = { ...DEFAULT_TRANSPORT, ...overrides };
  for (const [key, value] of Object.entries(config)) {
    if (typeof value === "number" && (!Number.isFinite(value) || value < 0 || value > 604800)) {
      throw new Error(`transport ${key} must be between 0 and 604800 seconds`);
    }
  }
  if (config.keepaliveIntervalSecs !== 0 && config.keepaliveTimeoutSecs === 0) {
    throw new Error("keepaliveTimeoutSecs must be positive when keepalive is enabled");
  }
  return config;
}

export function deadlineMiddleware(config: TransportConfig, ready?: () => Promise<void>): ClientMiddleware {
  return async function* (call, options) {
    if (options.signal?.aborted) throw new ClientError(call.method.path, Status.CANCELLED, "call cancelled");
    if (ready) {
      let abortReady: (() => void) | undefined;
      try {
        await Promise.race([
          ready(),
          new Promise<never>((_, reject) => {
            abortReady = () => reject(new ClientError(call.method.path, Status.CANCELLED, "call cancelled"));
            options.signal?.addEventListener("abort", abortReady, { once: true });
          }),
        ]);
      } finally {
        if (abortReady) options.signal?.removeEventListener("abort", abortReady);
      }
    }
    const method = call.method.path.split("/").at(-1);
    const longLived = ["WatchExec", "StreamExecLogs", "OpenExecSession", "OpenServiceTunnel", "OpenServiceDatagramTunnel"].includes(method ?? "");
    const download = method === "ReadFile";
    const seconds = longLived ? 0 : download ? config.progressTimeoutSecs
      : call.requestStream ? config.transferTimeoutSecs : config.rpcTimeoutSecs;
    const controller = new AbortController();
    const abort = () => controller.abort(options.signal?.reason);
    if (options.signal?.aborted) abort();
    else options.signal?.addEventListener("abort", abort, { once: true });
    let expired = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let progressTimer: ReturnType<typeof setTimeout> | undefined;
    const arm = (duration = seconds) => {
      if (duration === 0) return;
      timer = setTimeout(() => { expired = true; controller.abort(); }, duration * 1000);
      timer.unref?.();
    };
    const clear = () => { if (timer !== undefined) clearTimeout(timer); };
    const resetUploadProgress = () => {
      if (progressTimer !== undefined) clearTimeout(progressTimer);
      if (config.progressTimeoutSecs === 0) return;
      progressTimer = setTimeout(() => { expired = true; controller.abort(); }, config.progressTimeoutSecs * 1000);
      progressTimer.unref?.();
    };
    // gRPC permits at most eight digits; multi-day configurable deadlines
    // must use seconds rather than an overflowing millisecond field.
    const milliseconds = Math.ceil(seconds * 1000);
    const encodedTimeout = milliseconds <= 99999999 ? `${milliseconds}m` : `${Math.ceil(seconds)}S`;
    const metadata = download || longLived || seconds === 0 ? options.metadata
      : (options.metadata ?? Metadata()).set("grpc-timeout", encodedTimeout);
    const upload = call.requestStream && !longLived;
    const request = upload ? (async function* () {
      for await (const chunk of call.request as AsyncIterable<unknown>) {
        resetUploadProgress();
        yield chunk;
      }
    })() : call.request;
    const iterator = call.next(request as typeof call.request, {
      ...options, metadata, signal: controller.signal,
      onHeader(header) {
        if (longLived) clear();
        options.onHeader?.(header);
      },
    });
    try {
      if (longLived) arm(config.rpcTimeoutSecs);
      else if (!download) arm();
      if (upload) resetUploadProgress();
      while (true) {
        if (download) arm();
        const next = await iterator.next();
        if (download) clear();
        if (next.done) return next.value;
        yield next.value;
      }
    } catch (error) {
      if (expired) throw new ClientError(call.method.path, Status.DEADLINE_EXCEEDED, "gRPC progress deadline exceeded");
      throw error;
    } finally {
      clear();
      if (progressTimer !== undefined) clearTimeout(progressTimer);
      options.signal?.removeEventListener("abort", abort);
      // nice-grpc checks the signal during iterator cleanup. Graceful early
      // return (e.g. a terminal WatchExec event) must not become AbortError.
      try {
        await (iterator as AsyncGenerator<unknown, unknown, undefined>).return(undefined);
      } finally {
        controller.abort();
      }
    }
  };
}
