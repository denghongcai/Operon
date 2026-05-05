#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'USAGE'
Usage: scripts/ci/run-validations.sh [group]

Groups:
  core
  runtime
  sdk
  linux-system

When no group is provided, all validation scripts run in the stable order below.
USAGE
}

requested_group="${1:-all}"
case "$requested_group" in
  all|core|runtime|sdk|linux-system) ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    echo "unknown validation group: $requested_group" >&2
    usage >&2
    exit 2
    ;;
esac

validations=(
  "linux-system|v0.5 Docker Validation|scripts/verify-v0.5-docker.sh"
  "linux-system|v0.6 Linux Mount Validation|scripts/verify-v0.6-linux-mount.sh"
  "linux-system|v0.6.1 Linux Write Mount Validation|scripts/verify-v0.6.1-linux-write-mount.sh"
  "runtime|v0.6.2 CLI FS Cleanup Validation|scripts/verify-v0.6.2-cli-fs-cleanup.sh"
  "runtime|v0.6.3 FS Copy Validation|scripts/verify-v0.6.3-fs-copy.sh"
  "runtime|v0.6.4/v0.6.5 Onboard and Unified Config Validation|scripts/verify-v0.6.4-onboard.sh"
  "runtime|v0.6.7/v0.6.8/v0.6.12 Runtime and Protocol Validation|scripts/verify-v0.6.7-runtime.sh"
  "runtime|v0.6.9 CLI Contract Validation|scripts/verify-v0.6.9-cli-contract.sh"
  "runtime|v0.6.10 Runtime Hardening Validation|scripts/verify-v0.6.10-runtime-hardening.sh"
  "core|v0.6.11 Governance Validation|scripts/verify-v0.6.11-governance.sh"
  "sdk|v0.6.12 Runtime Boundary Validation|scripts/verify-v0.6.12-runtime-boundary.sh"
  "runtime|v0.7 Service Forwarding Validation|scripts/verify-v0.7-service-forwarding.sh"
  "runtime|v0.7.1 UDP Datagram Forwarding Validation|scripts/verify-v0.7.1-udp-datagram-forwarding.sh"
  "core|v0.8 Agent Skills Validation|scripts/verify-v0.8-agent-skills.sh"
  "runtime|v0.8.1 Integration Coverage Validation|scripts/verify-v0.8.1-integration-coverage.sh"
  "sdk|v0.8.3 Read Range and Release Cleanup Validation|scripts/verify-v0.8.3-read-range-release-cleanup.sh"
  "core|v0.8.4 Modularization Validation|scripts/verify-v0.8.4-modularization.sh"
  "core|v0.8.5 Core Domain Module Validation|scripts/verify-v0.8.5-core-domain-modules.sh"
  "sdk|v0.8.6 Runtime CLI Client Modularization Validation|scripts/verify-v0.8.6-runtime-cli-client-modularization.sh"
  "linux-system|Release GLIBC Baseline Validation|scripts/verify-release-glibc-baseline.sh"
  "core|Docs Help Skills Sync Validation|scripts/verify-docs-help-skills-sync.sh"
  "core|v0.9 Endpoint Model Validation|scripts/verify-v0.9-endpoint-model.sh"
  "core|Post-v0.9 Discovery UX Validation|scripts/verify-post-v0.9-discovery-ux.sh"
  "core|Policy-Derived Capability Validation|scripts/verify-policy-derived-capabilities.sh"
  "runtime|v0.9.3 Store-Backed Audit Visibility Validation|scripts/verify-v0.9.3-store-backed-audit-visibility.sh"
  "sdk|v0.9.4 Runtime Hardening Consolidation Validation|scripts/verify-v0.9.4-runtime-hardening-consolidation.sh"
  "core|v0.9.5 Policy Language Hardening Validation|scripts/verify-v0.9.5-policy-language-hardening.sh"
  "sdk|v0.9.6 Capability Diagnostics Validation|scripts/verify-v0.9.6-capability-diagnostics.sh"
  "sdk|v0.10 Exec Unification Validation|scripts/verify-v0.10-exec-unification.sh"
  "sdk|v0.10.1 Filesystem Consistency and Workspace Hardening Validation|scripts/verify-v0.10.1-fs-consistency-workspace-hardening.sh"
  "runtime|v0.10.2 Operator Diagnostics Validation|scripts/verify-v0.10.2-operator-diagnostics.sh"
  "runtime|v0.11 Exec Session Validation|scripts/verify-v0.11-exec-session.sh"
  "core|v0.10.4 Maintainability Cleanup Validation|scripts/verify-v0.10.4-maintainability-cleanup.sh"
  "core|v0.11.2 Exec Session Hardening Validation|scripts/verify-v0.11.2-exec-session-hardening.sh"
  "core|v0.10.5 Maintainability Cleanup Validation|scripts/verify-v0.10.5-maintainability-cleanup.sh"
  "core|v0.11.3 Platform Capability Matrix Validation|scripts/verify-v0.11.3-platform-capability-matrix.sh"
  "sdk|v0.12 Release Distribution Readiness Validation|scripts/verify-v0.12-release-distribution-readiness.sh"
  "core|v0.12.1 Platform Parity Hardening Validation|scripts/verify-v0.12.1-platform-parity-hardening.sh"
  "core|v0.12.2 Maintainability Cleanup Validation|scripts/verify-v0.12.2-maintainability-cleanup.sh"
  "runtime|v0.12.3 Windows Exec Process Tree Cancellation Validation|scripts/verify-v0.12.3-windows-exec-process-tree-cancellation.sh"
  "sdk|v0.12.4 Release Artifact Verification Validation|scripts/verify-v0.12.4-release-artifact-verification.sh"
  "core|v0.12.5 CLI gRPC Maintainability Cleanup Validation|scripts/verify-v0.12.5-cli-grpc-maintainability-cleanup.sh"
  "core|v0.13.4 CI Validation Consolidation|scripts/verify-v0.13.4-ci-validation-consolidation.sh"
  "core|v0.13.5 Daemon Service Management Validation|scripts/verify-v0.13.5-daemon-service-management.sh"
  "core|v0.13.6 Test Hardening Validation|scripts/verify-v0.13.6-test-hardening.sh"
  "core|v0.13.1 Windows PTY Validation|scripts/verify-v0.13.1-windows-pty-validation.sh"
  "core|v0.13.2 Windows Private File ACL Validation|scripts/verify-v0.13.2-windows-private-file-acl.sh"
  "core|v0.13.3 Config and Onboard Maintainability Validation|scripts/verify-v0.13.3-config-onboard-maintainability.sh"
  "core|v0.13.7 Mount Adapter Strategy Validation|scripts/verify-v0.13.7-mount-adapter-strategy.sh"
  "core|v0.13.8 Mount Core Boundary Validation|scripts/verify-v0.13.8-mount-core-boundary.sh"
  "core|v0.14 Cross-Platform Live Mount Validation|scripts/verify-v0.14-cross-platform-live-mount.sh"
  "core|v0.14.1 Mount Stabilization Validation|scripts/verify-v0.14.1-mount-stabilization.sh"
  "core|v0.15 Windows Exec Session Parity Validation|scripts/verify-v0.15-windows-exec-session-parity.sh"
)

failures=()
selected_count=0

for validation in "${validations[@]}"; do
  IFS="|" read -r group name script <<<"$validation"
  if [[ "$requested_group" != "all" && "$requested_group" != "$group" ]]; then
    continue
  fi

  selected_count=$((selected_count + 1))
  echo "::group::[$group] $name"
  echo "Running $script"
  started_at="$(date +%s)"
  if bash "$script"; then
    finished_at="$(date +%s)"
    echo "$name passed in $((finished_at - started_at))s"
  else
    status=$?
    finished_at="$(date +%s)"
    echo "::error title=$name failed::$script exited with status $status after $((finished_at - started_at))s"
    failures+=("[$group] $name | $script | exit $status")
  fi
  echo "::endgroup::"
done

if (( selected_count == 0 )); then
  echo "no validation scripts selected for group: $requested_group" >&2
  exit 2
fi

if (( ${#failures[@]} > 0 )); then
  echo "Validation failures:"
  for failure in "${failures[@]}"; do
    echo "- $failure"
  done
  exit 1
fi

echo "All $selected_count validation scripts passed for group: $requested_group"
