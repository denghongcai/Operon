#!/usr/bin/env bash
set -euo pipefail

case "${HOSTNAME:-}" in
  *node-a*)
    config=/etc/operon/node-a-config.yaml
    ;;
  *node-b*)
    config=/etc/operon/node-b-config.yaml
    ;;
  *)
    config=/etc/operon/node-a-config.yaml
    ;;
esac

if [[ "${1:-}" == "start" && "$*" != *"--config"* ]]; then
  exec operond "$@" --config "$config"
fi

exec operond "$@"
