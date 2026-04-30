export type NodeEndpoint = {
  nodeId: string;
  address: string;
  port: number;
  provider: "manual" | "cloudflare-mesh" | "tailscale" | "wireguard" | "ssh" | "lan" | "kubernetes";
};

export type OperonStep = {
  node: string;
  action: string;
  path?: string;
  command?: string;
};

export type OperonRunRequest = {
  steps: OperonStep[];
};
