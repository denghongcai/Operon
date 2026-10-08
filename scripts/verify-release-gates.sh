#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: scripts/verify-release-gates.sh <tag> <commit-sha> [owner/repo]

Checks release-only gates that cannot run in normal CI. For public release tags
this requires successful macOS FUSE-T and Windows WinFsp live mount smoke jobs
on the exact release commit.
USAGE
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

TAG="$1"
COMMIT_SHA="$2"
REPO="${3:-denghongcai/Operon}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/release-assets.sh"
WORKFLOW_NAMES=(
  "Cross-Platform Live Mount Smoke"
  "v0.14 Live Mount Smoke"
)
MACOS_JOB_NAMES=(
  "macOS FUSE-T Live Mount (hosted)"
  "macOS FUSE-T Live Mount (self-hosted)"
)
WINDOWS_JOB_NAMES=(
  "Windows WinFsp Live Mount"
)

if [[ "$TAG" != v* ]]; then
  echo "no public release gates for non-release tag $TAG"
  exit 0
fi
release_validate_tag "$TAG"
[[ "$COMMIT_SHA" =~ ^[0-9a-fA-F]{40}$ ]] || { echo 'release gates require a full exact commit SHA' >&2; exit 1; }

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required to verify release gates" >&2
  exit 1
fi

run_ids=()
for workflow_name in "${WORKFLOW_NAMES[@]}"; do
  while IFS= read -r run_id; do
    [[ -n "$run_id" ]] && run_ids+=("$run_id")
  done < <(
    gh run list \
      --repo "$REPO" \
      --workflow "$workflow_name" \
      --commit "$COMMIT_SHA" \
      --status success \
      --json databaseId \
      --jq '.[].databaseId' 2>/dev/null || true
  )
done

find_successful_job() {
  local gate_name="$1"
  shift
  local job_name run_id

  for run_id in "${run_ids[@]}"; do
    for job_name in "$@"; do
      if gh run view "$run_id" \
        --repo "$REPO" \
        --json jobs \
        --jq ".jobs[] | select(.name == \"$job_name\" and .conclusion == \"success\") | .name" \
        | grep -Fxq "$job_name"; then
        echo "$gate_name live mount release gate passed in workflow run $run_id ($job_name)"
        return 0
      fi
    done
  done

  echo "missing release gate: successful $gate_name live mount job in '${WORKFLOW_NAMES[0]}' on commit $COMMIT_SHA" >&2
  return 1
}

missing=0
find_successful_job "macOS FUSE-T" "${MACOS_JOB_NAMES[@]}" || missing=1
find_successful_job "Windows WinFsp" "${WINDOWS_JOB_NAMES[@]}" || missing=1

if release_has_musl_assets "$TAG"; then
  for architecture in x86_64 aarch64; do
    passed=false
    while IFS= read -r run_id; do
      [[ -n "$run_id" ]] || continue
      if gh run view "$run_id" --repo "$REPO" --json headSha,jobs --jq \
        ".headSha == \"$COMMIT_SHA\" and any(.jobs[]; .name == \"Native musl Build ($architecture)\" and .conclusion == \"success\" and any(.steps[]; .name == \"Native Alpine live FUSE and runtime acceptance\" and .conclusion == \"success\") and any(.steps[]; .name == \"Native rebooted Alpine OpenRC service acceptance\" and .conclusion == \"success\") and any(.steps[]; .name == \"Native identical-environment GNU and musl ops comparison\" and .conclusion == \"success\"))" \
        | grep -Fxq true; then
        echo "native $architecture Alpine runtime/FUSE/OpenRC gate passed in run $run_id"
        passed=true
        break
      fi
    done < <(gh run list --repo "$REPO" --workflow "Alpine musl Acceptance" --commit "$COMMIT_SHA" --status success --limit 20 --json databaseId --jq '.[].databaseId')
    if [[ "$passed" != true ]]; then
      echo "missing release gate: native $architecture Alpine runtime/FUSE/OpenRC on $COMMIT_SHA" >&2
      missing=1
    fi
  done
fi

if [[ "$missing" -ne 0 ]]; then
  echo "run '${WORKFLOW_NAMES[0]}' with platform=all, or run separate platform=macos and platform=windows dispatches, before creating or updating a public release" >&2
  echo "record the successful workflow run IDs in docs/plan/development-phases.md and the active release phase document" >&2
  exit 1
fi
