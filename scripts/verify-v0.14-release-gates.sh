#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: scripts/verify-v0.14-release-gates.sh <tag> <commit-sha> [owner/repo]

Checks release-only gates that cannot run in normal CI. For v0.14 tags this
requires a successful self-hosted macOS macFUSE live mount smoke on the exact
release commit.
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
MACOS_JOB_NAME="macOS macFUSE Live Mount (self-hosted)"

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
  if gh run view "$run_id" \
    --repo "$REPO" \
    --json jobs \
    --jq ".jobs[] | select(.name == \"$MACOS_JOB_NAME\" and .conclusion == \"success\") | .name" \
    | grep -Fxq "$MACOS_JOB_NAME"; then
    echo "v0.14 macOS live mount release gate passed in workflow run $run_id"
    exit 0
  fi
done

echo "missing v0.14 release gate: successful '$MACOS_JOB_NAME' in '$WORKFLOW_NAME' on commit $COMMIT_SHA" >&2
echo "run docs/plan/v0.14-macos-live-smoke-runbook.md before creating or updating the v0.14 release" >&2
exit 1
