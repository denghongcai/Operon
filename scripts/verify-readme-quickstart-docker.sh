#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  cat >&2 <<'USAGE'
usage:
  scripts/verify-readme-quickstart-docker.sh
  scripts/verify-readme-quickstart-docker.sh --dry-run

Runs the README Quickstart flow, including scripts/install.sh, in Docker
against OPERON_VERSION when set, or the latest public GitHub Release when unset. Dry-run mode prints the resolved
container image and release tag input without starting Docker.
USAGE
  exit 0
fi

IMAGE="${OPERON_README_IMAGE:-ubuntu:20.04}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "${1:-}" == "--dry-run" ]]; then
  echo "image=$IMAGE"
  echo "operon_version=${OPERON_VERSION:-latest-public-release}"
  exit 0
fi

docker run --rm -i \
  --name operon-readme-quickstart-validation \
  -e DEBIAN_FRONTEND=noninteractive \
  -e OPERON_VERSION="${OPERON_VERSION:-}" \
  -v "$ROOT/scripts/install.sh:/opt/operon/install.sh:ro" \
  "$IMAGE" \
  bash <<'CONTAINER'
set -euo pipefail

apt-get update >/dev/null
apt-get install -y --no-install-recommends \
  bash-completion \
  ca-certificates \
  coreutils \
  curl \
  git \
  procps \
  sudo \
  tar \
  zsh \
  >/dev/null

curl -fsSL https://deb.nodesource.com/setup_20.x | bash - >/dev/null
apt-get install -y --no-install-recommends nodejs >/dev/null

useradd -m -s /bin/bash operon
printf 'operon ALL=(ALL) NOPASSWD:ALL\n' >/etc/sudoers.d/operon
chmod 0440 /etc/sudoers.d/operon

cat >/tmp/verify-readme-user-flow.sh <<'USER_FLOW'
#!/usr/bin/env bash
set -euo pipefail

export HOME=/home/operon

VERSION="${OPERON_VERSION:-$(curl -fsSLI -o /dev/null -w '%{url_effective}' https://github.com/denghongcai/Operon/releases/latest)}"
VERSION="${VERSION##*/}"
test -n "$VERSION" || { echo "failed to resolve latest Operon release" >&2; exit 1; }
OPERON_VERSION="$VERSION" bash /opt/operon/install.sh

# New scheduling controls are part of the v0.16.13+ public CLI contract;
# historical releases remain usable by this generic Quickstart verifier.
if [[ "$(printf '%s\n' v0.16.13 "$VERSION" | sort -V | head -n 1)" == v0.16.13 ]]; then
  operon mount --help >/tmp/operon-mount-help.txt
  for flag in --mount-workers --read-concurrency --read-budget-mib; do
    grep -Fq -- "$flag" /tmp/operon-mount-help.txt
  done
fi

operond service --help >/tmp/operond-service-help.txt
operond service install --help >/tmp/operond-service-install-help.txt
if operond start --help | grep -q -- '--background'; then
  echo "operond start help unexpectedly exposes --background" >&2
  exit 1
fi

mkdir -p "$HOME/operon-workspace" "$HOME/.operon"
operon onboard \
  --yes \
  --role both \
  --output-dir "$HOME/.operon" \
  --node-id local \
  --workspace "$HOME/operon-workspace" \
  --listen 127.0.0.1:7789

operond start >/tmp/operond.log 2>&1 &
daemon_pid=$!
trap 'kill "$daemon_pid" >/dev/null 2>&1 || true; wait "$daemon_pid" >/dev/null 2>&1 || true' EXIT

for _ in $(seq 1 30); do
  if operon node ping local; then
    break
  fi
  sleep 1
done

operon capability list local

mkdir -p ~/.local/share/bash-completion/completions
operon completion bash > ~/.local/share/bash-completion/completions/operon
mkdir -p ~/.zfunc
operon completion zsh > ~/.zfunc/_operon

npx -y skills add https://github.com/denghongcai/Operon --list
npx -y skills add https://github.com/denghongcai/Operon --skill '*' --agent codex --yes

operon node list
operon node ping local
operon capability list local
operon capability explain local fs:workspace read /

operon fs list local:/
operon fs write local:/notes.txt --content "hello from Operon"
test "$(operon fs read local:/notes.txt)" = "hello from Operon"

operon exec run local -- echo hello
operon exec run local --argv -- printf "hello world"
operon exec list local
EXEC_ID="$(operon --json exec run local -- echo hello | sed -n 's/.*"id": "\([^"]*\)".*/\1/p' | head -n 1)"
test -n "$EXEC_ID"
operon exec logs local "$EXEC_ID"

operon service list local
operon service check local local-daemon
timeout 5s operon service forward local local-daemon --listen 127.0.0.1:17789 || test "$?" -eq 124

operon audit show local --limit 20

cat > ./workflow.yaml <<'YAML'
name: local-copy-and-run

steps:
  - id: write-input
    node: local
    action: fs.write
    path: /graph-input.txt
    content: hello from graph

  - id: run-command
    node: local
    action: exec.run
    cwd: /
    timeout_secs: 30
    command: cat graph-input.txt > graph-output.txt

  - id: read-output
    node: local
    action: fs.read
    path: /graph-output.txt
YAML

operon run --trace-output ./trace.json ./workflow.yaml
operon trace show ./trace.json

operon config explain
USER_FLOW

chmod +x /tmp/verify-readme-user-flow.sh
su - operon -c /tmp/verify-readme-user-flow.sh
CONTAINER

echo "README quickstart Docker validation passed"
