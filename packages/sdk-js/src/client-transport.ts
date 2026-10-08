import { createChannel, createClientFactory, waitForChannelReady, type CallOptions, type Channel } from "nice-grpc";

import { OperonRuntimeDefinition, type OperonRuntimeClient } from "./generated/operon/runtime";
import { grpcOptions, grpcTarget, type RequestContext } from "./transport";
import type { NodeEndpoint } from "./types";
import { deadlineMiddleware, transportConfig } from "./transport-deadlines";

export class GrpcClientPool {
  private readonly endpoints: Map<string, NodeEndpoint>;
  private readonly clients = new Map<
    string,
    { channel: Channel; client: OperonRuntimeClient }
  >();

  constructor(endpoints: NodeEndpoint[]) {
    this.endpoints = new Map(endpoints.map((endpoint) => [endpoint.nodeId, endpoint]));
  }

  close(): void {
    for (const { channel } of this.clients.values()) {
      channel.close();
    }
    this.clients.clear();
  }

  endpoint(nodeId: string): NodeEndpoint {
    const endpoint = this.endpoints.get(nodeId);
    if (!endpoint) {
      throw new Error(`node ${nodeId} not found`);
    }
    return endpoint;
  }

  client(endpoint: NodeEndpoint): OperonRuntimeClient {
    const cached = this.clients.get(endpoint.nodeId);
    if (cached) {
      return cached.client;
    }
    const config = transportConfig(endpoint.transport);
    const channel = createChannel(grpcTarget(endpoint.endpoint), undefined, {
      "grpc.max_receive_message_length": 16 * 1024 * 1024,
      "grpc.max_send_message_length": 16 * 1024 * 1024,
      "grpc.keepalive_time_ms": config.keepaliveIntervalSecs === 0 ? -1 : config.keepaliveIntervalSecs * 1000,
      "grpc.keepalive_timeout_ms": config.keepaliveTimeoutSecs * 1000,
      "grpc.keepalive_permit_without_calls": config.keepaliveWhileIdle ? 1 : 0,
    });
    const ready = config.connectTimeoutSecs === 0 ? undefined
      : () => waitForChannelReady(channel, new Date(Date.now() + config.connectTimeoutSecs * 1000));
    const client = createClientFactory().use(deadlineMiddleware(config, ready)).create(OperonRuntimeDefinition, channel);
    this.clients.set(endpoint.nodeId, { channel, client });
    return client;
  }

  options(endpoint: NodeEndpoint, context?: RequestContext): CallOptions {
    return grpcOptions(endpoint, context);
  }
}
