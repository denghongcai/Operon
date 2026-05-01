#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

docker compose up -d --build node-a node-b

cleanup() {
  docker compose down
}
trap cleanup EXIT

for node in node-a node-b; do
  echo "waiting for ${node}"
  for _ in $(seq 1 30); do
    if cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml node ping "$node" >/tmp/operon-"${node}".log 2>&1; then
      cat /tmp/operon-"${node}".log
      break
    fi
    sleep 1
  done

  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml node ping "$node"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml capability list "$node"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs write "$node:/hello.txt" --content "hello from ${node}"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs stat "$node:/hello.txt"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs list "$node:/"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs read "$node:/hello.txt"

  if cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs read "$node:/../etc/passwd" >/tmp/operon-"${node}"-escape.log 2>&1; then
    echo "expected path escape to fail for ${node}" >&2
    exit 1
  fi

  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job run "$node" -- echo "job from ${node}"

  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job run "$node" --detach --timeout-secs 10 -- sleep 5 >/tmp/operon-"${node}"-job.log
  cat /tmp/operon-"${node}"-job.log
  job_id="$(awk '{print $2}' /tmp/operon-"${node}"-job.log | head -n1)"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job cancel "$node" "$job_id"

  for _ in $(seq 1 30); do
    cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job status "$node" "$job_id" >/tmp/operon-"${node}"-job-status.log
    cat /tmp/operon-"${node}"-job-status.log
    if grep -Eq "Cancelled|TimedOut|Succeeded|Failed" /tmp/operon-"${node}"-job-status.log; then
      break
    fi
    sleep 1
  done

  if ! grep -q "Cancelled" /tmp/operon-"${node}"-job-status.log; then
    echo "expected cancelled job status for ${node}" >&2
    exit 1
  fi

  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job logs "$node" "$job_id"

  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job run "$node" --timeout-secs 1 -- sleep 5 >/tmp/operon-"${node}"-job-timeout.log
  cat /tmp/operon-"${node}"-job-timeout.log
  if ! grep -q "TimedOut" /tmp/operon-"${node}"-job-timeout.log; then
    echo "expected timed out job status for ${node}" >&2
    exit 1
  fi

  if cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job run "$node" --timeout-secs 31 -- echo denied >/tmp/operon-"${node}"-policy-deny.log 2>&1; then
    echo "expected job timeout policy denial for ${node}" >&2
    exit 1
  fi
  cat /tmp/operon-"${node}"-policy-deny.log

  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml audit list "$node"
done

cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml run examples/docker-copy-and-run.yaml
cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml node list
