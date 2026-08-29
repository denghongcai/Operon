import { createChannel, createClient, type CallOptions, type Channel } from "nice-grpc";

import { OperonRuntimeDefinition, type OperonRuntimeClient } from "./generated/operon/runtime";
import { grpcOptions, grpcTarget, type RequestContext } from "./transport";
import type { NodeEndpoint } from "./types";

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
    const channel = createChannel(grpcTarget(endpoint.endpoint));
    const client = createClient(OperonRuntimeDefinition, channel);
    this.clients.set(endpoint.nodeId, { channel, client });
    return client;
  }

  options(endpoint: NodeEndpoint, context?: RequestContext): CallOptions {
    return grpcOptions(endpoint, context);
  }
}
