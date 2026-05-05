#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: scripts/verify-v0.14-release-gates.sh <tag> <commit-sha> [owner/repo]

Checks release-only gates that cannot run in normal CI. For v0.14 tags this
requires a successful macOS FUSE-T live mount smoke on the exact release commit.
USAGE
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

TAG="$1"
COMMIT_SHA="$2"
REPO="${3:-denghongcai/Operon}"
WORKFLOW_NAME="v0.14 Live Mount Smoke"
MACOS_JOB_NAMES=(
  "macOS FUSE-T Live Mount (hosted)"
  "macOS FUSE-T Live Mount (self-hosted)"
)

if [[ "$TAG" != v0.14* ]]; then
  echo "no v0.14-specific release gates for $TAG"
  exit 0
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required to verify v0.14 release gates" >&2
  exit 1
fi

mapfile -t run_ids < <(
  gh run list \
    --repo "$REPO" \
    --workflow "$WORKFLOW_NAME" \
    --commit "$COMMIT_SHA" \
    --status success \
    --json databaseId \
    --jq '.[].databaseId'
)

for run_id in "${run_ids[@]}"; do
  for job_name in "${MACOS_JOB_NAMES[@]}"; do
    if gh run view "$run_id" \
      --repo "$REPO" \
      --json jobs \
      --jq ".jobs[] | select(.name == \"$job_name\" and .conclusion == \"success\") | .name" \
      | grep -Fxq "$job_name"; then
      echo "v0.14 macOS live mount release gate passed in workflow run $run_id"
      exit 0
    fi
  done
done

echo "missing v0.14 release gate: successful macOS FUSE-T live mount job in '$WORKFLOW_NAME' on commit $COMMIT_SHA" >&2
echo "run docs/plan/v0.14-macos-live-smoke-runbook.md before creating or updating the v0.14 release" >&2
exit 1
