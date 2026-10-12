# Operon TypeScript SDK

Use the Operon SDK to call remote Operon nodes from Node.js. It provides a
typed client for filesystem, execution, service, policy, audit, and workflow
operations.

## Install

```sh
npm install @cellworks/operon-sdk-js
```

**Requirements:** Node.js 22 or newer and a reachable Operon daemon.

## Quick start

Create a client with one or more node endpoints, make requests by `nodeId`, and
close the client when your application is done with it:

```ts
import { OperonClient } from "@cellworks/operon-sdk-js";

const client = new OperonClient([
  {
    nodeId: "remote",
    endpoint: "grpc://192.168.1.20:7789",
    token: process.env.OPERON_TOKEN,
  },
]);

try {
  const info = await client.statFs("remote", "/");
  console.log(info);
} finally {
  client.close();
}
```

The daemon must allow the requested operation through its authentication and
policy configuration. Keep bearer tokens in environment variables or a secret
manager; do not commit them to source control.

## Connection options

`grpc://` and `grpcs://` connect over your existing network, such as a LAN,
Tailscale, or WireGuard. This is the default and does not load native bindings.

For optional embedded peer-to-peer connections, use an `iroh://` endpoint and
configure the peer identity, routes, and bearer token. The daemon and SDK must
use matching versions. The connection guide covers the required configuration
and security details.

Bundled iroh bindings are available on these platforms:

| Platform | Architectures |
| --- | --- |
| Linux GNU/glibc | x86_64, arm64 |
| Linux musl | x86_64, arm64 |
| macOS | Intel, Apple Silicon |
| Windows | x86_64 |

ARMv7 has no bundled iroh binding. Ordinary gRPC use does not load native code.

## API and examples

The client exposes filesystem methods such as `statFs`, `listFs`,
`readFileBytes`, and `writeFileBytes`, along with execution, service, audit,
policy, and workflow methods. See the [TypeScript SDK example and API
reference](https://github.com/denghongcai/Operon/blob/main/DEVELOPMENT.md#typescript-sdk-example)
for more examples.

- [Embedded connection guide](https://github.com/denghongcai/Operon/blob/main/docs/networking/embedded-iroh.md)
- [Runtime API](https://github.com/denghongcai/Operon/blob/main/docs/architecture/runtime-api.md)
- [Repository and issue tracker](https://github.com/denghongcai/Operon)
