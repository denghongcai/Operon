// Actual public SDK + in-process binding acceptance against an owned daemon.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import net from "node:net";
import dgram from "node:dgram";
import { once } from "node:events";

const seconds = Number(process.env.OPERON_EMBEDDED_TEST_TIMEOUT_SECS ?? 120);
assert(Number.isFinite(seconds) && seconds > 0, "test timeout must be positive");
const bindingPath = process.env.OPERON_SDK_NATIVE_PATH;
assert(bindingPath && path.isAbsolute(bindingPath), "explicit absolute binding path required");
const binding = { exports: {} };
process.dlopen(binding, bindingPath);
const { NativeClient } = binding.exports;
const dir = await mkdtemp(path.join(os.tmpdir(), "operon-sdk-native-"));
let daemon;
let client;
let sdk;
let tcpEcho;
let udpEcho;
const echoSockets = new Set();
const limit = setTimeout(() => { daemon?.kill("SIGKILL"); process.exitCode = 1; }, seconds * 1000);
try {
  await mkdir(path.join(dir, "workspace"));
  tcpEcho = net.createServer(socket => {
    echoSockets.add(socket);
    socket.on("close", () => echoSockets.delete(socket));
    socket.on("data", data => socket.write(data));
  });
  tcpEcho.listen(0, "127.0.0.1");
  await once(tcpEcho, "listening");
  udpEcho = dgram.createSocket("udp4");
  udpEcho.on("message", (data, peer) => udpEcho.send(data, peer.port, peer.address));
  udpEcho.bind(0, "127.0.0.1");
  await once(udpEcho, "listening");
  // Keep the TCP lease until immediately before daemon startup. QUIC uses
  // its own dynamically selected localhost UDP port below.
  const lease = net.createServer();
  lease.listen(0, "127.0.0.1");
  await once(lease, "listening");
  const tcpPort = lease.address().port;
  const udpLease = dgram.createSocket("udp4");
  udpLease.bind(0, "127.0.0.1");
  await once(udpLease, "listening");
  const udpPort = udpLease.address().port;
  const configPath = path.join(dir, "config.yaml");
  await writeFile(configPath, JSON.stringify({ version: 1, daemon: {
    node_id: "native-sdk-owned", workspace: path.join(dir, "workspace"),
    grpc_listen: `127.0.0.1:${tcpPort}`, auth: { token: "native-sdk-test-token" },
    iroh: { identity_file: "server.key", bind_addrs: [`127.0.0.1:${udpPort}`] },
  }, policy: { subject: "sdk-acceptance", fs: { mounts: [{ name: "workspace", path: "/", permissions: { read: true, write: true, delete: true } }] },
    exec: { allowed_cwds: ["/"], default_timeout_secs: 30, max_timeout_secs: 300, allow_sessions: true, preserve_env: false, env_allowlist: [], allowed_secrets: [] },
    service: { services: [
      { id: "tcp-echo", name: "tcp echo", host: "127.0.0.1", port: tcpEcho.address().port, protocol: "tcp", description: "owned echo fixture", permissions: { check: true, forward: true } },
      { id: "udp-echo", name: "udp echo", host: "127.0.0.1", port: udpEcho.address().port, protocol: "udp", description: "owned datagram fixture", permissions: { check: true, forward: true } },
    ] } },
  }), { mode: 0o600 });
  await new Promise(resolve => lease.close(resolve));
  await new Promise(resolve => udpLease.close(resolve));
  daemon = spawn(process.env.OPERON_NATIVE_TEST_DAEMON ?? path.resolve("target/debug/operond"),
    ["start", "--config", configPath], { stdio: ["ignore", "pipe", "pipe"] });
  let logs = "";
  const endpointId = await new Promise((resolve, reject) => {
    const receive = chunk => {
      logs += chunk.toString().replace(/\x1b\[[0-9;]*m/g, "");
      const match = /endpoint_id=([a-f0-9]{64})/.exec(logs);
      if (match) resolve(match[1]);
    };
    daemon.stdout.on("data", receive);
    daemon.stderr.on("data", receive);
    daemon.once("error", reject);
    daemon.once("exit", code => reject(new Error(`native daemon exited (${code}): ${logs}`)));
  });
  const endpointConfig = { node_id: "logical-sdk-name",
    endpoint: `iroh://${endpointId}`, token: null, transport: { iroh: {
      identity_file: path.join(dir, "client.key"), bind_addrs: ["127.0.0.1:0"],
      peer_addrs: [`127.0.0.1:${udpPort}`],
    } } };
  client = await NativeClient.connect(JSON.stringify(endpointConfig));
  const rpc = async (name, token = "native-sdk-test-token") => {
    const call = await client.openCall(`/operon.runtime.v1.OperonRuntime/${name}`,
      JSON.stringify([["authorization", `Bearer ${token}`]]));
    try {
      await call.send(Buffer.alloc(0));
      await call.finishSend();
      const response = await call.receive();
      assert(Buffer.isBuffer(response) && response.length > 0, "missing protobuf response");
      assert.equal(await call.receive(), null, "unary RPC returned extra messages");
      return response;
    } finally { await call.close(); }
  };
  const health = await rpc("Health");
  assert(health.includes(Buffer.from("v0.")), "health response has no protocol version");
  const node = await rpc("GetNode");
  assert(node.includes(Buffer.from("native-sdk-owned")), "node response has wrong logical ID");
  await assert.rejects(rpc("Health", "invalid-test-token"), /grpc-status:16:/);
  const stalled = await client.openCall("/operon.runtime.v1.OperonRuntime/Health", "[]");
  const waiting = stalled.receive();
  stalled.cancel();
  await assert.rejects(waiting, /cancelled/);
  await stalled.close();
  const unreachable = structuredClone(endpointConfig);
  unreachable.transport.connect_timeout_secs = 0;
  unreachable.transport.iroh.identity_file = path.join(dir, "cancelled-client.key");
  unreachable.transport.iroh.peer_addrs = ["127.0.0.1:1"];
  const pendingClient = new NativeClient(JSON.stringify(unreachable));
  const pendingReady = pendingClient.ready();
  pendingClient.close();
  await assert.rejects(pendingReady, /closed/);
  client.close();
  await assert.rejects(client.openCall("/operon.runtime.v1.OperonRuntime/Health", "[]"), /closed/);
  const { OperonClient } = await import("../packages/sdk-js/dist/index.js");
  sdk = new OperonClient([{ nodeId: "sdk-logical", endpoint: `iroh://${endpointId}`, token: "native-sdk-test-token",
    transport: { iroh: { identityFile: path.join(dir, "public-sdk.key"), bindAddrs: ["127.0.0.1:0"], peerAddrs: [`127.0.0.1:${udpPort}`] } } }]);
  const capabilities = await sdk.listCapabilities("sdk-logical");
  assert(capabilities.capabilities.length > 0);
  const payload = Uint8Array.from({ length: 8 * 1024 * 1024 + 117 }, (_, i) => i % 251);
  await sdk.writeFileBytes("sdk-logical", "/large.bin", payload);
  const received = new Uint8Array(await sdk.readFileBytes("sdk-logical", "/large.bin"));
  assert.deepEqual(received, payload);
  const range = await sdk.readFileRangeBytes("sdk-logical", "/large.bin", 123, 4 * 1024 * 1024);
  assert.deepEqual(new Uint8Array(range), payload.subarray(123, 123 + 4 * 1024 * 1024));
  await assert.rejects(sdk.statFs("sdk-logical", "/../escape"));
  const exec = await sdk.runExec("sdk-logical", { command: process.platform === "win32" ? "echo sdk-native-exec" : "printf sdk-native-exec", cwd: "/" });
  for await (const event of await sdk.watchExec("sdk-logical", exec.id)) { /* drain terminal event stream */ }
  assert.equal((await sdk.getExec("sdk-logical", exec.id)).exit_code, 0);
  const logsStream = await sdk.streamExecLogs("sdk-logical", exec.id);
  assert((await new Response(logsStream).text()).includes("sdk-native-exec"));
  const echoed = Buffer.from("native-sdk-tcp-echo");
  let finishTcp;
  const tcpInputDone = new Promise(resolve => { finishTcp = resolve; });
  const tunnel = await sdk.openServiceTunnel("sdk-logical", "tcp-echo", (async function* () { yield echoed; await tcpInputDone; })());
  const reader = tunnel.getReader();
  const tcpResponse = await reader.read();
  assert.deepEqual(Buffer.from(tcpResponse.value), echoed);
  await reader.cancel();
  finishTcp();
  let finishUdp;
  const udpInputDone = new Promise(resolve => { finishUdp = resolve; });
  const datagrams = await sdk.openServiceDatagramTunnel("sdk-logical", "udp-echo", (async function* () {
    yield { peer_id: "owned-peer", data: echoed };
    await udpInputDone;
  })());
  const datagramIterator = datagrams[Symbol.asyncIterator]();
  while (true) {
    const next = await datagramIterator.next();
    assert(!next.done, "datagram stream ended before echo");
    const event = next.value;
    if (event.type === "datagram") {
      assert.equal(event.peer_id, "owned-peer");
      assert.deepEqual(Buffer.from(event.data), echoed);
      break;
    }
  }
  const pendingDatagram = assert.rejects(datagramIterator.next(), error => error.code === 1);
  await datagramIterator.return();
  await pendingDatagram;
  finishUdp();
  const session = await sdk.openExecSession("sdk-logical", { command: process.platform === "win32" ? "echo native-sdk-pty" : "printf native-sdk-pty", cwd: "/" });
  const output = [];
  let sawExit = false;
  for await (const event of session) {
    if (event.type === "output") output.push(Buffer.from(event.data));
    if (event.type === "exit") { assert.equal(event.exit_code, 0); sawExit = true; }
  }
  assert(sawExit && Buffer.concat(output).toString().includes("native-sdk-pty"));
  console.log("PASS public SDK native TCP/UDP bidirectional echo, stream cancellation and PTY output/exit");
  console.log("PASS public SDK native unary, client/server streaming, >8MiB roundtrip, 4MiB range and traversal rejection");
  console.log("PASS in-process Node binding, native daemon health/node, bearer rejection, cancellation and joined call shutdown");
} finally {
  client?.close();
  sdk?.close();
  if (daemon && daemon.exitCode === null && daemon.signalCode === null) {
    daemon.kill("SIGTERM");
    await once(daemon, "exit");
  }
  clearTimeout(limit);
  for (const socket of echoSockets) socket.destroy();
  if (tcpEcho?.listening) await new Promise(resolve => tcpEcho.close(resolve));
  if (udpEcho) await new Promise(resolve => udpEcho.close(resolve));
  await rm(dir, { recursive: true, force: true });
}
