#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/release-gate-orchestrate.sh plan <tag> <commit-sha> [owner/repo]
  scripts/release-gate-orchestrate.sh pretag <tag> <commit-sha> [owner/repo]
  scripts/release-gate-orchestrate.sh postrelease <tag> <commit-sha> [owner/repo]

Coordinates the release gate sequence for a public Operon release.

Modes:
  plan         Print the gate order and exact manual commands.
  pretag       Verify CI, CodeQL, live mount release gates, and Windows runner
               image smoke have passed on the exact release commit.
  postrelease  Verify the draft release run, public release state, release
               artifact verification, release install usability, and README
               Quickstart verification have passed.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

MODE="${1:-}"
TAG="${2:-}"
COMMIT_SHA="${3:-}"
REPO="${4:-${GITHUB_REPOSITORY:-}}"

if [[ -z "$MODE" || -z "$TAG" || -z "$COMMIT_SHA" ]]; then
  usage
  exit 2
fi

if [[ -z "$REPO" ]]; then
  if remote_url="$(git remote get-url origin 2>/dev/null)"; then
    REPO="$(printf '%s\n' "$remote_url" \
      | sed -E 's#^git@github.com:##; s#^https://github.com/##; s#\.git$##')"
  fi
fi

if [[ -z "$REPO" ]]; then
  echo "failed to determine GitHub repository; pass owner/repo explicitly" >&2
  exit 1
fi

require_public_tag() {
  if [[ "$TAG" != v* ]]; then
    echo "no public release orchestration gates for non-release tag $TAG"
    exit 0
  fi
}

require_gh() {
  if ! command -v gh >/dev/null 2>&1; then
    echo "gh is required for release gate orchestration" >&2
    exit 1
  fi
}

successful_run_ids() {
  local workflow_name="$1"
  gh run list \
    --repo "$REPO" \
    --workflow "$workflow_name" \
    --commit "$COMMIT_SHA" \
    --status success \
    --limit 20 \
    --json databaseId \
    --jq '.[].databaseId' 2>/dev/null || true
}

require_successful_workflow() {
  local workflow_name="$1"
  local run_id

  run_id="$(successful_run_ids "$workflow_name" | head -n 1)"
  if [[ -z "$run_id" ]]; then
    echo "missing successful workflow '$workflow_name' on commit $COMMIT_SHA" >&2
    exit 1
  fi

  echo "$workflow_name passed in workflow run $run_id"
}

require_successful_job() {
  local workflow_name="$1"
  local gate_name="$2"
  shift 2
  local run_id job_name

  while IFS= read -r run_id; do
    [[ -n "$run_id" ]] || continue
    for job_name in "$@"; do
      if gh run view "$run_id" \
        --repo "$REPO" \
        --json jobs \
        --jq ".jobs[] | select(.name == \"$job_name\" and .conclusion == \"success\") | .name" \
        | grep -Fxq "$job_name"; then
        echo "$gate_name passed in workflow run $run_id ($job_name)"
        return 0
      fi
    done
  done < <(successful_run_ids "$workflow_name")

  echo "missing successful $gate_name job in workflow '$workflow_name' on commit $COMMIT_SHA" >&2
  exit 1
}

print_plan() {
  cat <<PLAN
Release gate orchestration plan for $REPO@$TAG on commit $COMMIT_SHA

1. Push the release-preparation commit to main and wait for:
   gh run list --repo "$REPO" --workflow "CI" --commit "$COMMIT_SHA"
   gh run list --repo "$REPO" --workflow "CodeQL" --commit "$COMMIT_SHA"

2. Dispatch release-only pre-tag gates on the exact commit:
   gh workflow run "Cross-Platform Live Mount Smoke" --repo "$REPO" --ref main -f platform=all -f macos_backend=nfs -f macos_runner=hosted
   gh workflow run "Windows Runner Image Smoke" --repo "$REPO" --ref main -f runner_label=windows-2025 -f release_tag=v0.16.6

3. Verify pre-tag gates:
   scripts/release-gate-orchestrate.sh pretag "$TAG" "$COMMIT_SHA" "$REPO"

4. Create and push the public release tag:
   git tag "$TAG" "$COMMIT_SHA"
   git push origin "$TAG"

5. Wait for the Draft Release workflow, inspect assets, then publish:
   gh run list --repo "$REPO" --workflow "Draft Release" --commit "$COMMIT_SHA"
   gh release edit "$TAG" --repo "$REPO" --draft=false

6. Dispatch post-publication verification:
   gh workflow run "Verify Release Artifacts" --repo "$REPO" --ref main -f tag="$TAG"
   gh workflow run "Verify Release Install Usability" --repo "$REPO" --ref main -f tag="$TAG"
   gh workflow run "Verify README Quickstart" --repo "$REPO" --ref main -f tag="$TAG"

7. Verify post-release gates and record run IDs in the active phase docs:
   scripts/release-gate-orchestrate.sh postrelease "$TAG" "$COMMIT_SHA" "$REPO"
PLAN
}

verify_pretag() {
  require_public_tag
  require_gh

  require_successful_workflow "CI"
  require_successful_workflow "CodeQL"
  scripts/verify-release-gates.sh "$TAG" "$COMMIT_SHA" "$REPO"
  require_successful_job \
    "Windows Runner Image Smoke" \
    "Windows Runner Image Smoke" \
    "Windows Runner Image Smoke (windows-2025)"

  echo "pre-tag release gates passed for $REPO@$TAG on $COMMIT_SHA"
}

verify_postrelease() {
  local is_draft is_prerelease release_url

  require_public_tag
  require_gh

  require_successful_workflow "Draft Release"

  is_draft="$(gh release view "$TAG" --repo "$REPO" --json isDraft --jq '.isDraft')"
  is_prerelease="$(gh release view "$TAG" --repo "$REPO" --json isPrerelease --jq '.isPrerelease')"
  release_url="$(gh release view "$TAG" --repo "$REPO" --json url --jq '.url')"

  if [[ "$is_draft" != "false" ]]; then
    echo "release $TAG is still a draft" >&2
    exit 1
  fi
  if [[ "$is_prerelease" != "false" ]]; then
    echo "release $TAG is unexpectedly marked as prerelease" >&2
    exit 1
  fi

  echo "public release is published: $release_url"
  require_successful_workflow "Verify Release Artifacts"
  require_successful_workflow "Verify Release Install Usability"
  require_successful_workflow "Verify README Quickstart"

  echo "post-release verification gates passed for $REPO@$TAG on $COMMIT_SHA"
}

case "$MODE" in
  plan)
    print_plan
    ;;
  pretag)
    verify_pretag
    ;;
  postrelease)
    verify_postrelease
    ;;
  *)
    usage
    exit 2
    ;;
esac
