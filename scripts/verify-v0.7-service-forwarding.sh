#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "v0.7 service forwarding validation currently requires Linux" >&2
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
WEB_DIR="$TMP_DIR/web"
CONFIG_PATH="$TMP_DIR/config.yaml"
STORE_PATH="$TMP_DIR/store.jsonl"
DAEMON_PORT="18873"
SERVICE_PORT="18874"
FORWARD_PORT="18875"

mkdir -p "$WORKSPACE_DIR" "$WEB_DIR"
printf 'operon service forwarding\n' >"$WEB_DIR/index.html"
rg -n 'rpc OpenServiceTunnel\(stream ServiceTunnelRequest\) returns \(stream ServiceTunnelResponse\)' proto/operon/runtime.proto >/dev/null
rg -n 'PROTOCOL_VERSION: &str = "v0.14.0"' crates/operon-protocol/src/lib.rs >/dev/null

python3 -m http.server "$SERVICE_PORT" --bind 127.0.0.1 --directory "$WEB_DIR" \
  >"$TMP_DIR/service.log" 2>&1 &
SERVICE_PID="$!"

for _ in $(seq 1 50); do
  if python3 - "$SERVICE_PORT" <<'PY' >/dev/null 2>&1
import socket
import sys

with socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=0.2) as sock:
    sock.sendall(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")
    assert b"operon service forwarding" in sock.recv(4096)
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
  exec:
    allowed_cwds:
      - /
    default_timeout_secs: 30
    max_timeout_secs: 60
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
  service:
    services:
      - id: web
        name: local-web
        host: 127.0.0.1
        port: $SERVICE_PORT
        protocol: tcp
        description: local test web server
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
grep -q "web" "$TMP_DIR/services.txt"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" service check local web \
  >"$TMP_DIR/service-check.txt"
grep -q "ok=true" "$TMP_DIR/service-check.txt"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" service forward local web \
  --listen 127.0.0.1:$FORWARD_PORT >"$TMP_DIR/forward.log" 2>&1 &
FORWARD_PID="$!"

for _ in $(seq 1 50); do
  if python3 - "$FORWARD_PORT" <<'PY' >"$TMP_DIR/forward-response.txt" 2>/dev/null
import socket
import sys

with socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=0.5) as sock:
    sock.sendall(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")
    payload = b""
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            break
        payload += chunk
print(payload.decode("utf-8", "replace"))
assert b"operon service forwarding" in payload
PY
  then
    break
  fi
  sleep 0.1
done

grep -q "operon service forwarding" "$TMP_DIR/forward-response.txt"
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" --json audit show local \
  --capability service:web \
  --action forward \
  --allowed true \
  --limit 1 \
  >"$TMP_DIR/service-forward-audit.json"
python3 - "$TMP_DIR/service-forward-audit.json" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    audit = json.load(handle)
events = audit["events"]
assert len(events) == 1, events
event = events[0]
assert event["capability"] == "service:web", event
assert event["action"] == "forward", event
assert event["allowed"] is True, event
PY

echo "v0.7 service forwarding validation passed"
