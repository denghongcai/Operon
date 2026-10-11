#!/usr/bin/env bash
set -euo pipefail

# First relay gate: real owned Docker relay, no IP transports on either peer.
# Distinct-container/NAT/recovery and real-daemon gates remain separate.
image=${OPERON_RELAY_TEST_IMAGE:-operon-iroh-relay-acceptance}
timeout_secs=${OPERON_EMBEDDED_TEST_TIMEOUT_SECS:-120}
[[ "$timeout_secs" =~ ^[1-9][0-9]*$ ]] || { echo 'test timeout must be positive' >&2; exit 1; }
container_id=$(docker run -d --label operon.acceptance=embedded-relay \
  --publish 127.0.0.1::3340 --publish 127.0.0.1::9090 "$image")
[[ "$container_id" =~ ^[a-f0-9]{64}$ ]] || { echo 'invalid owned container ID' >&2; exit 1; }
cleanup() { docker rm -f "$container_id" >/dev/null; }
trap cleanup EXIT
relay_mapping=$(docker port "$container_id" 3340/tcp)
metrics_mapping=$(docker port "$container_id" 9090/tcp)
relay_port=${relay_mapping##*:}
metrics_port=${metrics_mapping##*:}
[[ "$relay_mapping" == 127.0.0.1:* && "$metrics_mapping" == 127.0.0.1:* ]] || exit 1
deadline=$((SECONDS + timeout_secs))
until curl --fail --silent --max-time 2 "http://127.0.0.1:$metrics_port/metrics" >/dev/null; do
  if [[ $(docker inspect --format '{{.State.Running}}' "$container_id") != true || $SECONDS -ge $deadline ]]; then
    docker logs "$container_id"
    echo 'owned relay failed readiness' >&2
    exit 1
  fi
  sleep 0.2
done
OPERON_TEST_RELAY_URL="http://127.0.0.1:$relay_port" \
  OPERON_EMBEDDED_TEST_TIMEOUT_SECS="$timeout_secs" \
  cargo test -p operon-transport --features iroh --test relay --locked -- --ignored --nocapture
if [[ -n "${OPERON_SDK_NATIVE_PATH:-}" ]]; then
  OPERON_SDK_TEST_RELAY_URL="http://127.0.0.1:$relay_port" \
    node --expose-gc scripts/verify-sdk-native.mjs
fi
if [[ "${OPERON_TEST_EMBEDDED_MOUNT:-0}" == 1 ]]; then
  OPERON_MOUNT_TEST_RELAY_URL="http://127.0.0.1:$relay_port" \
    python3 scripts/verify-embedded-mount.py
fi
metrics=$(curl --fail --silent --max-time 10 "http://127.0.0.1:$metrics_port/metrics")
for counter in relayserver_bytes_sent_total relayserver_bytes_recv_total; do
  value=$(awk -v key="$counter" '$1 == key { print $2 }' <<<"$metrics")
  awk -v value="$value" 'BEGIN { exit !(value + 0 > 8388608) }' || {
    echo "insufficient owned relay traffic: $counter=$value" >&2
    exit 1
  }
  echo "$counter=$value"
done
echo 'PASS: owned Docker relay, relay-only >8MiB streams and actual relay counters'
