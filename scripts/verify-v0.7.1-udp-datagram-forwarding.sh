#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "v0.7.1 UDP datagram forwarding validation currently requires Linux" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
DAEMON_PID=""
SERVICE_PID=""
FORWARD_PID=""
cleanup() {
  if [[ -n "$FORWARD_PID" ]]; then
    kill "$FORWARD_PID" >/dev/null 2>&1 || true
    wait "$FORWARD_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$DAEMON_PID" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$SERVICE_PID" ]]; then
    kill "$SERVICE_PID" >/dev/null 2>&1 || true
    wait "$SERVICE_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

WORKSPACE_DIR="$TMP_DIR/workspace"
CONFIG_PATH="$TMP_DIR/config.yaml"
STORE_PATH="$TMP_DIR/store.jsonl"
DAEMON_PORT="18876"
SERVICE_PORT="18877"
FORWARD_PORT="18878"

mkdir -p "$WORKSPACE_DIR"
rg -n 'rpc OpenServiceDatagramTunnel\(stream ServiceDatagramTunnelRequest\) returns \(stream ServiceDatagramTunnelResponse\)' proto/operon/runtime.proto >/dev/null
rg -n 'SERVICE_PROTOCOL_UDP' proto/operon/runtime.proto >/dev/null

python3 - "$SERVICE_PORT" >"$TMP_DIR/udp-service.log" 2>&1 <<'PY' &
import socket
import sys

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("127.0.0.1", int(sys.argv[1])))
while True:
    data, addr = sock.recvfrom(65535)
    sock.sendto(b"echo:" + data, addr)
PY
SERVICE_PID="$!"

for _ in $(seq 1 50); do
  if python3 - "$SERVICE_PORT" <<'PY' >/dev/null 2>&1
import socket
import sys

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.settimeout(0.2)
sock.sendto(b"ready", ("127.0.0.1", int(sys.argv[1])))
data, _ = sock.recvfrom(65535)
assert data == b"echo:ready", data
PY
  then
    break
  fi
  sleep 0.1
done

cat >"$CONFIG_PATH" <<YAML
version: 1

daemon:
  node_id: local
  grpc_listen: 127.0.0.1:$DAEMON_PORT
  workspace: $WORKSPACE_DIR
  advertise_lan: false
  store: $STORE_PATH

client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:$DAEMON_PORT

policy:
  subject: local-cli
  fs:
    mounts:
      - name: workspace
        path: /
        permissions:
          read: true
          write: true
          delete: true
  job:
    allowed_cwds:
      - /
    default_timeout_secs: 30
    max_timeout_secs: 60
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
  service:
    services:
      - id: echo-udp
        name: echo-udp
        host: 127.0.0.1
        port: $SERVICE_PORT
        protocol: udp
        description: local UDP echo service
        permissions:
          check: true
          forward: true
YAML

cargo run -q -p operond -- start --config "$CONFIG_PATH" >"$TMP_DIR/operond.log" 2>&1 &
DAEMON_PID="$!"

for _ in $(seq 1 50); do
  if cargo run -q -p operon-cli -- --config "$CONFIG_PATH" node ping local >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" service list local \
  >"$TMP_DIR/services.txt"
grep -q "echo-udp" "$TMP_DIR/services.txt"
grep -q "udp" "$TMP_DIR/services.txt"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" service check local echo-udp \
  >"$TMP_DIR/service-check.txt"
grep -q "ok=true" "$TMP_DIR/service-check.txt"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" service forward-udp local echo-udp \
  --listen 127.0.0.1:$FORWARD_PORT >"$TMP_DIR/forward-udp.log" 2>&1 &
FORWARD_PID="$!"

for _ in $(seq 1 50); do
  if python3 - "$FORWARD_PORT" <<'PY' >"$TMP_DIR/forward-udp-response.txt" 2>/dev/null
import socket
import sys

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.settimeout(0.5)
target = ("127.0.0.1", int(sys.argv[1]))
for payload in (b"one", b"two"):
    sock.sendto(payload, target)
    data, _ = sock.recvfrom(65535)
    print(data.decode("utf-8"))
    assert data == b"echo:" + payload, data
PY
  then
    break
  fi
  sleep 0.1
done

grep -q "echo:one" "$TMP_DIR/forward-udp-response.txt"
grep -q "echo:two" "$TMP_DIR/forward-udp-response.txt"
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" --json audit show local \
  --capability service:echo-udp \
  --action forward-udp \
  --allowed true \
  --limit 1 \
  >"$TMP_DIR/service-forward-udp-audit.json"
python3 - "$TMP_DIR/service-forward-udp-audit.json" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    audit = json.load(handle)
events = audit["events"]
assert len(events) == 1, events
event = events[0]
assert event["capability"] == "service:echo-udp", event
assert event["action"] == "forward-udp", event
assert event["allowed"] is True, event
PY

echo "v0.7.1 UDP datagram forwarding validation passed"
