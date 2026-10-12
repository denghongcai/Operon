# Operon TypeScript SDK

```sh
npm install @cellworks/operon-sdk-js
```

```ts
import { OperonClient } from "@cellworks/operon-sdk-js";

const client = new OperonClient([{
  nodeId: "remote",
  endpoint: "grpc://192.168.1.20:7789",
  token: process.env.OPERON_TOKEN,
}]);

try {
  console.log(await client.statFs("remote", "/"));
} finally {
  client.close();
}
```

Node.js 22 or newer. Existing LAN/VPN gRPC connections remain the default.
Optional `iroh://` connections use bundled native bindings on Linux GNU/musl
x86_64/arm64, macOS Intel/ARM and Windows x86_64. No native code is loaded for
ordinary gRPC. ARMv7 has no bundled iroh binding. Client identities, explicit
peer addresses/relays and bearer authentication must still be configured.

See the [connection guide](https://github.com/denghongcai/Operon/blob/main/docs/networking/embedded-iroh.md)
and [API documentation](https://github.com/denghongcai/Operon/blob/main/DEVELOPMENT.md#typescript-sdk-example).
