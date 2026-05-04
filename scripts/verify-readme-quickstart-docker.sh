#!/usr/bin/env bash
set -euo pipefail

IMAGE="${OPERON_README_IMAGE:-ubuntu:20.04}"

docker run --rm -i \
  --name operon-readme-quickstart-validation \
  -e DEBIAN_FRONTEND=noninteractive \
  -e OPERON_VERSION="${OPERON_VERSION:-}" \
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

VERSION="${OPERON_VERSION:-$(curl -fsSL https://api.github.com/repos/denghongcai/Operon/releases/latest | sed -n 's/.*"tag_name": "\(v[^"]*\)".*/\1/p')}"
test -n "$VERSION" || { echo "failed to resolve latest Operon release" >&2; exit 1; }
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ARCH=linux-x86_64 ;;
  Linux-aarch64|Linux-arm64) ARCH=linux-arm64 ;;
  Linux-armv7l|Linux-armv7*) ARCH=linux-armv7 ;;
  Darwin-x86_64) ARCH=macos-x86_64 ;;
  Darwin-arm64) ARCH=macos-aarch64 ;;
  *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

curl -fsSL "https://github.com/denghongcai/Operon/releases/download/${VERSION}/operon-${VERSION}-${ARCH}.tar.gz" -o /tmp/operon.tar.gz
tar -xzf /tmp/operon.tar.gz -C /tmp
sudo install "/tmp/operon-${VERSION}-${ARCH}/operon" /usr/local/bin/operon
sudo install "/tmp/operon-${VERSION}-${ARCH}/operond" /usr/local/bin/operond

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
