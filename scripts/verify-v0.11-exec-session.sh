#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_pattern 'rpc OpenExecSession\(stream ExecSessionRequest\) returns \(stream ExecSessionEvent\)' proto/operon/runtime.proto
require_pattern 'message ExecSessionStart' proto/operon/runtime.proto
require_pattern 'message ExecSessionInput' proto/operon/runtime.proto
require_pattern 'message ExecSessionResize' proto/operon/runtime.proto
require_pattern 'message ExecSessionEvent' proto/operon/runtime.proto
require_pattern 'allow_sessions' crates/operon-core/src/policy.rs
require_pattern 'exec.session' docs/plan/v0.11-exec-session-pty-interactive.md
require_pattern 'Session' crates/operon-cli/src/cli_args.rs
require_pattern 'RawModeGuard' crates/operon-cli/src/commands/exec_session.rs
require_pattern 'ExecSessionInputSource::LocalStdin' crates/operon-cli/src/commands/exec_session.rs
require_pattern 'open_exec_session' crates/operond/src
require_pattern 'openExecSession' packages/sdk-js/src/index.ts

cargo test -p operon-process --locked exec_session_policy
cargo test -p operon-protocol --locked exec_session
cargo test -p operon-cli --locked clap_model_exposes_exec_session_command

tmpdir="$(mktemp -d)"
daemon_pid=""
cleanup() {
  if [[ -n "$daemon_pid" ]]; then
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
  fi
  rm -rf "$tmpdir"
}
trap cleanup EXIT

port="$(python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
)"
token="$tmpdir/token"
config="$tmpdir/config.yaml"
workspace="$tmpdir/workspace"
store="$tmpdir/store.jsonl"
printf 'test-token\n' >"$token"
chmod 600 "$token"
mkdir -p "$workspace"
cat >"$config" <<YAML
version: 1
daemon:
  node_id: local
  grpc_listen: 127.0.0.1:$port
  workspace: $workspace
  store: $store
  auth:
    token_file: $token
client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:$port
      auth:
        token_file: $token
policy:
  subject: local-cli
  fs:
    mounts: []
  exec:
    allowed_cwds:
      - /
    default_timeout_secs: 30
    max_timeout_secs: 60
    allow_sessions: true
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
YAML

cargo run -q -p operond -- start --config "$config" >"$tmpdir/daemon.log" 2>&1 &
daemon_pid="$!"
for _ in $(seq 1 60); do
  if cargo run -q -p operon-cli -- --config "$config" node ping local >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
cargo run -q -p operon-cli -- --config "$config" node ping local >/dev/null
cargo run -q -p operon-cli -- --config "$config" capability explain local exec:default session / \
  | grep -q 'allowed=true'
session_output="$(cargo run -q -p operon-cli -- --config "$config" exec session local --timeout-secs 30 --argv -- /bin/sh -lc 'printf session-ok' 2>&1)"
grep -q 'session-ok' <<<"$session_output"
grep -q 'session Succeeded' <<<"$session_output"

echo "v0.11 exec session validation passed"
