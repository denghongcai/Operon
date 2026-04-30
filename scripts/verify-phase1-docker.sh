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
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml audit list "$node"
done

cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml node list
