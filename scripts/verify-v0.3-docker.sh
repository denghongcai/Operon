#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

docker compose up -d --build node-a node-b

cat >/tmp/operon-docker-nodes-no-token.yaml <<'YAML'
nodes:
  node-a:
    endpoint: http://127.0.0.1:17788
  node-b:
    endpoint: http://127.0.0.1:17789
YAML

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
  cargo run -q -p operon-cli -- --json --config examples/docker-nodes.yaml node ping "$node" >/tmp/operon-"${node}"-ping.json
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml node resolve "$node"
  if cargo run -q -p operon-cli -- --config /tmp/operon-docker-nodes-no-token.yaml node ping "$node" >/tmp/operon-"${node}"-unauthorized.log 2>&1; then
    echo "expected unauthorized node ping to fail for ${node}" >&2
    exit 1
  fi
  cat /tmp/operon-"${node}"-unauthorized.log
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml capability list "$node"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs write "$node:/hello.txt" --content "hello from ${node}"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs stat "$node:/hello.txt"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs list "$node:/"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs read "$node:/hello.txt"
  printf "streamed from %s" "$node" >/tmp/operon-"${node}"-stream-input.txt
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs write "$node:/stream.txt" --file /tmp/operon-"${node}"-stream-input.txt
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs read "$node:/stream.txt" --output /tmp/operon-"${node}"-stream-output.txt
  cmp /tmp/operon-"${node}"-stream-input.txt /tmp/operon-"${node}"-stream-output.txt

  if cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs read "$node:/../etc/passwd" >/tmp/operon-"${node}"-escape.log 2>&1; then
    echo "expected path escape to fail for ${node}" >&2
    exit 1
  fi

  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job run "$node" -- echo "job from ${node}"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job run "$node" --secret OPERON_TEST_SECRET -- test x'$OPERON_TEST_SECRET' = xdocker-secret

  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job run "$node" --detach --timeout-secs 10 -- "cat > stdin.txt" >/tmp/operon-"${node}"-stdin-job.log
  cat /tmp/operon-"${node}"-stdin-job.log
  stdin_job_id="$(awk '{print $2}' /tmp/operon-"${node}"-stdin-job.log | head -n1)"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job stdin "$node" "$stdin_job_id" --content "stdin from ${node}"
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job stdin "$node" "$stdin_job_id" --close
  for _ in $(seq 1 30); do
    cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job status "$node" "$stdin_job_id" >/tmp/operon-"${node}"-stdin-status.log
    cat /tmp/operon-"${node}"-stdin-status.log
    if grep -q "Succeeded" /tmp/operon-"${node}"-stdin-status.log; then
      break
    fi
    sleep 1
  done
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs read "$node:/stdin.txt" --output /tmp/operon-"${node}"-stdin-output.txt
  grep -q "stdin from ${node}" /tmp/operon-"${node}"-stdin-output.txt

  if cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job run "$node" --secret DENIED_SECRET -- echo denied >/tmp/operon-"${node}"-secret-deny.log 2>&1; then
    echo "expected denied secret policy failure for ${node}" >&2
    exit 1
  fi
  cat /tmp/operon-"${node}"-secret-deny.log

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
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job logs "$node" "$job_id" --follow
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job logs "$node" "$job_id" --stream

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
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml audit show "$node" --limit 5
  cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml job list "$node"
  docker compose exec -T "$node" test -s /workspace/operon-store.jsonl
done

cargo run -q -p operon-cli -- provider list
cargo run -q -p operon-cli -- init config /tmp/operon-init-nodes.yaml
cargo run -q -p operon-cli -- init policy /tmp/operon-init-policy.yaml
test -s /tmp/operon-init-nodes.yaml
test -s /tmp/operon-init-policy.yaml
docker compose exec -T node-a operon node discover --provider lan --timeout-secs 2 >/tmp/operon-lan-discover.log
cat /tmp/operon-lan-discover.log
grep -q "node-" /tmp/operon-lan-discover.log
cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml run --trace-output /tmp/operon-docker-trace.json examples/docker-copy-and-run.yaml
cargo run -q -p operon-cli -- trace show /tmp/operon-docker-trace.json
cargo run -q -p operon-cli -- trace list /tmp
cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml mount read-only node-a:/ --to /tmp/operon-mount-poc
test -s /tmp/operon-mount-poc/.operon-mount.json
cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml node list
