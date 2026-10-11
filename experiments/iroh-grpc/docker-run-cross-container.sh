#!/usr/bin/env bash
# Cross-container iroh gRPC PoC: server and client containers on an isolated
# Docker bridge network, direct IP dialing with no relay or discovery.
set -euo pipefail
cd "$(dirname "$0")"

IMAGE="${OPERON_POC_IMAGE:-operon-iroh-grpc-poc}"
TIMEOUT="${OPERON_POC_TEST_TIMEOUT_SECS:-60}"
[[ "$TIMEOUT" =~ ^[1-9][0-9]*$ ]] || { echo 'test timeout must be a positive integer' >&2; exit 2; }
TASK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/operon-iroh-poc.XXXXXX")
NET="operon-iroh-poc-${TASK_DIR##*.}"
SERVER="${NET}-server"
CLIENT="${NET}-client"
NETWORK_ID=""

cleanup() {
    for cidfile in "$TASK_DIR/server.cid" "$TASK_DIR/client.cid"; do
        if [[ -s "$cidfile" ]]; then
            local owned_id
            owned_id=$(<"$cidfile")
            [[ "$owned_id" =~ ^[a-f0-9]{64}$ ]] && docker rm -f "$owned_id" >/dev/null 2>&1 || true
        fi
    done
    [[ -z "$NETWORK_ID" ]] || docker network rm "$NETWORK_ID" >/dev/null 2>&1 || true
    # Preserve cidfiles on failure for diagnostics; never delete by shared name.
}
trap cleanup EXIT

NETWORK_ID=$(docker network create --driver bridge "$NET")
docker run -d --cidfile "$TASK_DIR/server.cid" --name "$SERVER" --network "$NETWORK_ID" "$IMAGE" iroh_server >/dev/null

node_id=""
addr=""
for _ in $(seq 1 50); do
    node_id=$(docker logs "$SERVER" 2>/dev/null | sed -n 's/^IROH_NODE_ID=//p' | head -1)
    addr=$(docker logs "$SERVER" 2>/dev/null | sed -n 's/^IROH_IP_ADDR=//p' | head -1)
    [ -n "$node_id" ] && [ -n "$addr" ] && break
    sleep 0.2
done
[ -n "$node_id" ] && [ -n "$addr" ] || { docker logs "$SERVER"; exit 1; }
echo "server node id: $node_id"
echo "server direct addr: $addr"

timeout --foreground "$TIMEOUT" docker run --cidfile "$TASK_DIR/client.cid" --name "$CLIENT" --network "$NETWORK_ID" "$IMAGE" \
    iroh_client "$node_id" "$addr"
docker kill --signal INT "$SERVER" >/dev/null
server_status=$(timeout --foreground "$TIMEOUT" docker wait "$SERVER")
[[ "$server_status" == "0" ]] || { docker logs "$SERVER"; exit 1; }
docker logs "$SERVER" 2>&1 | grep -F 'PASS server accept tasks joined'
