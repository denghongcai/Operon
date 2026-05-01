export type NetworkProvider =
  | "manual"
  | "cloudflare-mesh"
  | "tailscale"
  | "wireguard"
  | "ssh"
  | "lan"
  | "kubernetes";

export type NodeEndpoint = {
  nodeId: string;
  endpoint: string;
  provider?: NetworkProvider;
};

export type OperonStep = {
  id?: string;
  node: string;
  action: "fs.stat" | "fs.list" | "fs.read" | "fs.write" | "job.run";
  path?: string;
  content?: string;
  command?: string;
  cwd?: string;
  timeoutSecs?: number;
};

export type OperonRunRequest = {
  name: string;
  steps: OperonStep[];
};

export type OperonRunStatus = "running" | "succeeded" | "failed";

export type OperonTrace = {
  runId: string;
  name: string;
  status: OperonRunStatus;
  steps: OperonStepTrace[];
};

export type OperonStepTrace = {
  id: string;
  node: string;
  action: string;
  status: OperonRunStatus;
  startedAtMs: number;
  endedAtMs: number;
  error?: string;
  output?: unknown;
};

type JobRecord = {
  id: string;
  node_id: string;
  command: string;
  cwd: string;
  status: "running" | "succeeded" | "failed" | "cancelled" | "timed-out";
  exit_code?: number | null;
  logs: Array<{ stream: string; data: string }>;
};

export class OperonClient {
  private readonly endpoints: Map<string, NodeEndpoint>;

  constructor(endpoints: NodeEndpoint[]) {
    this.endpoints = new Map(endpoints.map((endpoint) => [endpoint.nodeId, endpoint]));
  }

  async run(request: OperonRunRequest): Promise<OperonTrace> {
    const trace: OperonTrace = {
      runId: `run-${Date.now()}`,
      name: request.name,
      status: "running",
      steps: [],
    };

    for (const [index, step] of request.steps.entries()) {
      const stepTrace = await this.runStep(step, index);
      trace.steps.push(stepTrace);

      if (stepTrace.status === "failed") {
        trace.status = "failed";
        return trace;
      }
    }

    trace.status = "succeeded";
    return trace;
  }

  private async runStep(step: OperonStep, index: number): Promise<OperonStepTrace> {
    const startedAtMs = Date.now();
    const id = step.id ?? `step-${index + 1}`;

    try {
      const output = await this.runAction(step);
      return {
        id,
        node: step.node,
        action: step.action,
        status: "succeeded",
        startedAtMs,
        endedAtMs: Date.now(),
        output,
      };
    } catch (error) {
      return {
        id,
        node: step.node,
        action: step.action,
        status: "failed",
        startedAtMs,
        endedAtMs: Date.now(),
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private async runAction(step: OperonStep): Promise<unknown> {
    switch (step.action) {
      case "fs.stat":
        return this.get(step.node, `/fs/stat?path=${encodeURIComponent(required(step.path, "path"))}`);
      case "fs.list":
        return this.get(step.node, `/fs/list?path=${encodeURIComponent(required(step.path, "path"))}`);
      case "fs.read":
        return this.get(step.node, `/fs/read?path=${encodeURIComponent(required(step.path, "path"))}`);
      case "fs.write":
        return this.post(step.node, "/fs/write", {
          path: required(step.path, "path"),
          content: step.content ?? "",
        });
      case "job.run":
        return this.runJob(step);
    }
  }

  private async runJob(step: OperonStep): Promise<JobRecord> {
    const job = await this.post<JobRecord>(step.node, "/job/run", {
      command: required(step.command, "command"),
      cwd: step.cwd,
      timeout_secs: step.timeoutSecs,
    });

    while (true) {
      const record = await this.get<JobRecord>(step.node, `/job/status?id=${encodeURIComponent(job.id)}`);
      if (record.status === "running") {
        await sleep(100);
        continue;
      }
      if (record.status === "succeeded") {
        return record;
      }
      throw new Error(`job ${record.id} ended with status ${record.status}`);
    }
  }

  private async get<T = unknown>(nodeId: string, path: string): Promise<T> {
    return this.request<T>(nodeId, path, { method: "GET" });
  }

  private async post<T = unknown>(nodeId: string, path: string, body: unknown): Promise<T> {
    return this.request<T>(nodeId, path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  private async request<T>(nodeId: string, path: string, init: RequestInit): Promise<T> {
    const endpoint = this.endpoints.get(nodeId);
    if (!endpoint) {
      throw new Error(`node ${nodeId} not found`);
    }

    const response = await fetch(new URL(path, endpoint.endpoint), init);
    if (!response.ok) {
      throw new Error(`request to ${endpoint.endpoint}${path} failed: ${response.status} ${response.statusText}`);
    }
    return response.json() as Promise<T>;
  }
}

function required(value: string | undefined, field: string): string {
  if (!value) {
    throw new Error(`step requires ${field}`);
  }
  return value;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
