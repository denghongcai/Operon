#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

cleanup() {
  docker compose down
}
trap cleanup EXIT

docker compose up -d --build node-a node-b

cat >/tmp/operon-docker-grpc-no-token.yaml <<'YAML'
version: 1
client:
  nodes:
    node-a:
      endpoint: grpc://127.0.0.1:17790
    node-b:
      endpoint: grpc://127.0.0.1:17791
YAML

for node in node-a node-b; do
  echo "waiting for ${node} gRPC"
  for _ in $(seq 1 30); do
    if cargo run -q -p operon-cli -- --config examples/docker-config.yaml node ping "$node" >/tmp/operon-"${node}"-grpc.log 2>&1; then
      cat /tmp/operon-"${node}"-grpc.log
      break
    fi
    sleep 1
  done

  cargo run -q -p operon-cli -- --config examples/docker-config.yaml node ping "$node"
  cargo run -q -p operon-cli -- --json --config examples/docker-config.yaml node ping "$node" >/tmp/operon-"${node}"-grpc-ping.json
  grep -q '"node"' /tmp/operon-"${node}"-grpc-ping.json

  if cargo run -q -p operon-cli -- --config /tmp/operon-docker-grpc-no-token.yaml node ping "$node" >/tmp/operon-"${node}"-grpc-unauthorized.log 2>&1; then
    echo "expected unauthorized gRPC node ping to fail for ${node}" >&2
    exit 1
  fi
  cat /tmp/operon-"${node}"-grpc-unauthorized.log

  cargo run -q -p operon-cli -- --config examples/docker-config.yaml capability list "$node"
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml service list "$node"
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml service check "$node" daemon
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml fs write "$node:/hello-grpc.txt" --content "hello from ${node} grpc"
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml fs stat "$node:/hello-grpc.txt"
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml fs list "$node:/"
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml fs read "$node:/hello-grpc.txt" >/tmp/operon-"${node}"-grpc-read.txt
  grep -q "hello from ${node} grpc" /tmp/operon-"${node}"-grpc-read.txt

  printf "streamed from %s over grpc" "$node" >/tmp/operon-"${node}"-grpc-stream-input.txt
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml fs write "$node:/stream-grpc.txt" --file /tmp/operon-"${node}"-grpc-stream-input.txt
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml fs read "$node:/stream-grpc.txt" --output /tmp/operon-"${node}"-grpc-stream-output.txt
  cmp /tmp/operon-"${node}"-grpc-stream-input.txt /tmp/operon-"${node}"-grpc-stream-output.txt

  cargo run -q -p operon-cli -- --config examples/docker-config.yaml job run "$node" -- echo "job from ${node} grpc"
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml job run "$node" --secret OPERON_TEST_SECRET -- 'test "x$OPERON_TEST_SECRET" = xdocker-secret'

  cargo run -q -p operon-cli -- --config examples/docker-config.yaml job run "$node" --detach --timeout-secs 10 -- "cat > stdin-grpc.txt" >/tmp/operon-"${node}"-grpc-stdin-job.log
  stdin_job_id="$(awk '{print $2}' /tmp/operon-"${node}"-grpc-stdin-job.log | head -n1)"
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml job stdin "$node" "$stdin_job_id" --content "stdin from ${node} grpc"
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml job stdin "$node" "$stdin_job_id" --close
  for _ in $(seq 1 30); do
    cargo run -q -p operon-cli -- --config examples/docker-config.yaml job status "$node" "$stdin_job_id" >/tmp/operon-"${node}"-grpc-stdin-status.log
    cat /tmp/operon-"${node}"-grpc-stdin-status.log
    if grep -q "Succeeded" /tmp/operon-"${node}"-grpc-stdin-status.log; then
      break
    fi
    sleep 1
  done
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml fs read "$node:/stdin-grpc.txt" --output /tmp/operon-"${node}"-grpc-stdin-output.txt
  grep -q "stdin from ${node} grpc" /tmp/operon-"${node}"-grpc-stdin-output.txt

  if cargo run -q -p operon-cli -- --config examples/docker-config.yaml job run "$node" --secret DENIED_SECRET -- echo denied >/tmp/operon-"${node}"-grpc-secret-deny.log 2>&1; then
    echo "expected denied gRPC secret policy failure for ${node}" >&2
    exit 1
  fi
  cat /tmp/operon-"${node}"-grpc-secret-deny.log

  cargo run -q -p operon-cli -- --config examples/docker-config.yaml audit list "$node"
  cargo run -q -p operon-cli -- --config examples/docker-config.yaml job list "$node"
done

cargo run -q -p operon-cli -- --config examples/docker-config.yaml node ping node-a
cargo run -q -p operon-cli -- --config examples/docker-config.yaml run --trace-output /tmp/operon-docker-grpc-trace.json examples/docker-copy-and-run.yaml
cargo run -q -p operon-cli -- trace show /tmp/operon-docker-grpc-trace.json
