#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! find skills -mindepth 2 -maxdepth 2 -name SKILL.md | grep -q .; then
  echo "no repo-local skills found under skills/*/SKILL.md" >&2
  exit 1
fi

while IFS= read -r skill; do
  python - "$skill" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
if not text.startswith("---\n"):
    raise SystemExit(f"{path}: missing YAML frontmatter")
parts = text.split("---\n", 2)
if len(parts) < 3:
    raise SystemExit(f"{path}: incomplete YAML frontmatter")
frontmatter = parts[1]
if "name:" not in frontmatter:
    raise SystemExit(f"{path}: missing frontmatter name")
if "description:" not in frontmatter:
    raise SystemExit(f"{path}: missing frontmatter description")
if "mcp" in text.lower():
    raise SystemExit(f"{path}: skills must not reference MCP as an Operon runtime surface")
PY
done < <(find skills -mindepth 2 -maxdepth 2 -name SKILL.md | sort)

combined="$(mktemp)"
tmpdir=""
trap 'rm -f "$combined"; if [[ -n "$tmpdir" ]]; then rm -rf "$tmpdir"; fi' EXIT
find skills -mindepth 2 -maxdepth 2 -name SKILL.md -print0 | sort -z | xargs -0 cat > "$combined"

for required in \
  "operon config explain" \
  "--help" \
  "operon service forward" \
  "operon service forward-udp" \
  "audit" \
  "trace" \
  "policy" \
  "Confirm"; do
  if ! grep -q -- "$required" "$combined"; then
    echo "skills are missing required guidance: $required" >&2
    exit 1
  fi
done

help_commands=(
  "--help"
  "config --help"
  "config explain --help"
  "node --help"
  "node list --help"
  "node discover --help"
  "node resolve --help"
  "node ping --help"
  "init --help"
  "init config --help"
  "onboard --help"
  "capability --help"
  "capability list --help"
  "fs --help"
  "fs stat --help"
  "fs list --help"
  "fs read --help"
  "fs write --help"
  "fs mkdir --help"
  "fs rm --help"
  "fs rename --help"
  "fs copy --help"
  "fs truncate --help"
  "audit --help"
  "audit list --help"
  "audit show --help"
  "service --help"
  "service list --help"
  "service check --help"
  "service forward --help"
  "service forward-udp --help"
  "job --help"
  "job run --help"
  "job list --help"
  "job status --help"
  "job logs --help"
  "job stdin --help"
  "job cancel --help"
  "run --help"
  "graph --help"
  "graph run --help"
  "workflow --help"
  "workflow run --help"
  "trace --help"
  "trace show --help"
  "trace list --help"
  "mount --help"
  "completion --help"
)

for command in "${help_commands[@]}"; do
  if ! output="$(cargo run -q -p operon-cli -- $command 2>&1)"; then
    echo "help command failed: operon $command" >&2
    echo "$output" >&2
    exit 1
  fi
  if ! grep -Eq "Usage:|Commands:|Options:" <<<"$output"; then
    echo "help command did not look like clap help: operon $command" >&2
    echo "$output" >&2
    exit 1
  fi
done

tmpdir="$(mktemp -d)"
cargo run -q -p operon-cli -- --quiet init config "$tmpdir/config.yaml"
cargo run -q -p operon-cli -- --config "$tmpdir/config.yaml" --json config explain \
  | python -c '
import json
import sys

data = json.load(sys.stdin)
assert data["daemon"]["node_id"] == "local"
assert data["daemon"]["auth"].startswith("token_file:")
assert data["client"]["nodes"][0]["node_id"] == "local"
assert data["policy"]["fs_mounts"][0]["name"] == "workspace"
assert data["policy"]["services"][0]["protocol"] == "tcp"
assert data["secrets"]["file"].endswith("secrets.yaml")
'

cargo run -q -p operon-cli -- completion bash | grep -q "complete -F"
cargo run -q -p operon-cli -- completion zsh | grep -q "#compdef operon"
cargo run -q -p operon-cli -- onboard --yes --output-dir "$tmpdir/onboard" \
  | grep -q "operon completion zsh"

echo "v0.8 agent skills validation passed"
