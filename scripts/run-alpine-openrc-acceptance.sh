#!/usr/bin/env bash
set -euo pipefail
# Boot Alpine's real BusyBox PID 1 -> OpenRC sysinit/boot/default chain.
# All system-service mutations are confined to this owned, disposable container.
[[ $# == 2 ]] || { echo "usage: $0 <binary-directory> <pinned-alpine-image>" >&2; exit 2; }
deadline="${OPERON_OPENRC_ACCEPT_TIMEOUT_SECS:-120}"
stop_deadline="${OPERON_OPENRC_STOP_TIMEOUT_SECS:-30}"
for value in "$deadline" "$stop_deadline"; do
  [[ "$value" =~ ^[0-9]{1,6}$ ]] && (( 10#$value <= 604800 )) || { echo 'fixture deadlines must be integers from 0 to 604800' >&2; exit 2; }
done
deadline=$((10#$deadline))
stop_deadline=$((10#$stop_deadline))
binary_directory="$(realpath "$1")"
repository="$(cd "$(dirname "$0")/.." && pwd)"
for executable in operon operond; do test -x "$binary_directory/$executable"; done
fixture="$(mktemp -d /tmp/operon-openrc-container.XXXXXX)"
mkdir "$fixture/binaries"
cp "$binary_directory/operon" "$binary_directory/operond" "$fixture/binaries/"
sha256sum "$fixture/binaries/operon" "$fixture/binaries/operond" >"$fixture/binaries.sha256"
cat "$fixture/binaries.sha256"
# Freeze the evidence binaries: concurrent local rebuilds must not replace a
# running daemon's executable inode or contaminate supervisor identity checks.
binary_directory="$fixture/binaries"
container="$(basename "$fixture")"
cleanup() {
  docker logs "$container" >"$fixture/init.log" 2>&1 || true
  if docker inspect "$container" >/dev/null 2>&1; then
    docker rm -f "$container" >/dev/null || { echo "failed to remove owned fixture $container" >&2; return 1; }
  fi
  rm -f "$fixture/binaries/operon" "$fixture/binaries/operond"
  rmdir "$fixture/binaries"
  echo "OpenRC fixture boot log: $fixture/init.log"
}
trap cleanup EXIT
docker run -d --tty --name "$container" --tmpfs /run --cap-add SYS_ADMIN --security-opt apparmor=unconfined \
  --mount "type=bind,src=$binary_directory,dst=/operon,readonly" \
  --mount "type=bind,src=$repository,dst=/workspace,readonly" \
  -e OPERON_OPENRC_ACCEPT_TIMEOUT_SECS="${OPERON_OPENRC_ACCEPT_TIMEOUT_SECS:-120}" \
  -e OPERON_OPENRC_STOP_TIMEOUT_SECS="${OPERON_OPENRC_STOP_TIMEOUT_SECS:-30}" \
  "$2" /bin/sh -ec 'apk add --no-cache openrc python3 >/dev/null; exec /sbin/init' >/dev/null
start_seconds=$SECONDS
until docker exec "$container" /bin/sh -ec 'test "$(cat /proc/1/comm)" = init; test "$(cat /run/openrc/softlevel)" = default' >/dev/null 2>&1; do
  test "$(docker inspect -f '{{.State.Running}}' "$container")" = true || { echo 'Alpine init exited' >&2; exit 1; }
  if (( deadline != 0 && SECONDS - start_seconds >= deadline )); then
    echo "Alpine OpenRC boot timed out; adjust OPERON_OPENRC_ACCEPT_TIMEOUT_SECS" >&2; exit 1
  fi
  sleep 0.1
done
docker exec "$container" python3 /workspace/scripts/alpine-openrc-acceptance.py --bin-dir /operon
restart_deadline="$stop_deadline"
if (( restart_deadline == 0 )); then restart_deadline=-1; fi
docker restart --time "$restart_deadline" "$container" >/dev/null
start_seconds=$SECONDS
until docker exec "$container" /bin/sh -ec 'test "$(cat /proc/1/comm)" = init; test "$(cat /run/openrc/softlevel)" = default' >/dev/null 2>&1; do
  test "$(docker inspect -f '{{.State.Running}}' "$container")" = true || { echo 'Alpine init exited after reboot' >&2; exit 1; }
  if (( deadline != 0 && SECONDS - start_seconds >= deadline )); then
    echo 'Alpine OpenRC reboot timed out' >&2; exit 1
  fi
  sleep 0.1
done
docker exec "$container" python3 /workspace/scripts/alpine-openrc-acceptance.py --bin-dir /operon --after-reboot
