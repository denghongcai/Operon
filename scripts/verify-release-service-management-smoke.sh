#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/verify-release-service-management-smoke.sh <tag> [owner/repo]
  scripts/verify-release-service-management-smoke.sh --dry-run <tag> [owner/repo]

Downloads the current platform's public Operon release archive, installs the
release binaries into an isolated prefix, and verifies operond service
management commands against safe fake platform supervisors.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=true
  shift
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/release-install.sh
source "$ROOT/scripts/lib/release-install.sh"

TAG="${1:-}"
REPO="${2:-${GITHUB_REPOSITORY:-}}"

if [[ -z "$TAG" ]]; then
  usage
  exit 1
fi

REPO="$(release_install_repo_from_remote "$REPO")"
if [[ -z "$REPO" ]]; then
  echo "failed to determine GitHub repository; pass owner/repo explicitly" >&2
  exit 1
fi

asset="$(release_install_current_asset_name "$TAG")"

if [[ "$DRY_RUN" == true ]]; then
  echo "repo=$REPO"
  echo "tag=$TAG"
  echo "asset=$asset"
  echo "install_prefix=\${OPERON_RELEASE_INSTALL_PREFIX:-temporary-prefix}"
  echo "fake systemctl/launchctl/sc.exe supervisor smoke"
  echo "operond service install --config \${HOME}/.operon/config.yaml"
  echo "operond service start"
  echo "operond service status"
  echo "operond service stop"
  echo "operond service uninstall"
  exit 0
fi

cleanup() {
  rm -rf "$RELEASE_INSTALL_WORKDIR"
}
trap cleanup EXIT

release_install_setup "$TAG" "$REPO"

operond service --help >/dev/null
operond service install --help >/dev/null
operond service start --help >/dev/null
operond service stop --help >/dev/null
operond service status --help >/dev/null
operond service uninstall --help >/dev/null

workspace="$HOME/operon-workspace"
mkdir -p "$workspace" "$HOME/.operon"
operon onboard \
  --yes \
  --role both \
  --output-dir "$HOME/.operon" \
  --node-id local \
  --workspace "$workspace" \
  --listen "127.0.0.1:17789" \
  >/dev/null
config="$HOME/.operon/config.yaml"
test -f "$config" || { echo "missing generated config: $config" >&2; exit 1; }

fake_bin="$RELEASE_INSTALL_WORKDIR/fake-bin"
mkdir -p "$fake_bin"
supervisor_log="$RELEASE_INSTALL_WORKDIR/supervisor.log"
: >"$supervisor_log"
export OPERON_FAKE_SUPERVISOR_LOG="$supervisor_log"
export PATH="$fake_bin:$PATH"

write_fake_unix_supervisor() {
  local path="$1"
  cat >"$path" <<'SH'
#!/usr/bin/env bash
printf '%s' "$(basename "$0")" >> "${OPERON_FAKE_SUPERVISOR_LOG:?}"
for arg in "$@"; do
  printf ' %s' "$arg" >> "$OPERON_FAKE_SUPERVISOR_LOG"
done
printf '\n' >> "$OPERON_FAKE_SUPERVISOR_LOG"
exit 0
SH
  chmod +x "$path"
}

write_fake_windows_sc() {
  local fake_sc="$RELEASE_INSTALL_PREFIX_BIN/sc.exe"
  local fake_sc_windows="$fake_sc"
  local log_windows="$supervisor_log"
  if command -v cygpath >/dev/null 2>&1; then
    fake_sc_windows="$(cygpath -w "$fake_sc")"
    log_windows="$(cygpath -w "$supervisor_log")"
  fi

  export OPERON_FAKE_SC_EXE="$fake_sc_windows"
  export OPERON_FAKE_SUPERVISOR_LOG="$log_windows"
  if command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -ExecutionPolicy Bypass -Command '
$source = @"
using System;
using System.IO;
public static class FakeSc {
  public static int Main(string[] args) {
    string log = Environment.GetEnvironmentVariable("OPERON_FAKE_SUPERVISOR_LOG");
    File.AppendAllText(log, "sc.exe " + String.Join(" ", args) + Environment.NewLine);
    return 0;
  }
}
"@
Add-Type -TypeDefinition $source -OutputAssembly $env:OPERON_FAKE_SC_EXE -OutputType ConsoleApplication
'
  elif command -v pwsh >/dev/null 2>&1; then
    pwsh -NoProfile -Command '
$source = @"
using System;
using System.IO;
public static class FakeSc {
  public static int Main(string[] args) {
    string log = Environment.GetEnvironmentVariable("OPERON_FAKE_SUPERVISOR_LOG");
    File.AppendAllText(log, "sc.exe " + String.Join(" ", args) + Environment.NewLine);
    return 0;
  }
}
"@
Add-Type -TypeDefinition $source -OutputAssembly $env:OPERON_FAKE_SC_EXE -OutputType ConsoleApplication
'
  else
    echo "PowerShell is required to build fake sc.exe for Windows service smoke" >&2
    exit 1
  fi
}

assert_contains() {
  local needle="$1"
  local file="$2"
  if ! grep -Fq "$needle" "$file"; then
    echo "missing expected content in $file: $needle" >&2
    sed -n '1,160p' "$file" >&2 || true
    exit 1
  fi
}

assert_contains_any() {
  local file="$1"
  shift

  local needle
  for needle in "$@"; do
    if grep -Fq "$needle" "$file"; then
      return 0
    fi
  done

  echo "missing expected content in $file; tried:" >&2
  for needle in "$@"; do
    echo "  $needle" >&2
  done
  sed -n '1,160p' "$file" >&2 || true
  exit 1
}

macos_private_var_alias() {
  local path="$1"
  if [[ "$path" == /private/var/* ]]; then
    printf '%s\n' "${path#/private}"
  elif [[ "$path" == /var/* ]]; then
    printf '/private%s\n' "$path"
  else
    printf '%s\n' "$path"
  fi
}

run_service_commands() {
  operond service install --config "$config"
  operond service start
  operond service status
  operond service stop
  operond service uninstall
}

case "$(uname -s)" in
  Linux*)
    write_fake_unix_supervisor "$fake_bin/systemctl"
    export XDG_CONFIG_HOME="$RELEASE_INSTALL_WORKDIR/xdg-config"
    unit_path="$XDG_CONFIG_HOME/systemd/user/operond.service"
    operond service install --config "$config"
    test -f "$unit_path" || { echo "missing generated systemd unit: $unit_path" >&2; exit 1; }
    assert_contains "ExecStart=$RELEASE_INSTALL_PREFIX_BIN/operond start --config $config" "$unit_path"
    operond service start
    operond service status
    operond service stop
    operond service uninstall
    test ! -e "$unit_path" || { echo "systemd unit still exists after uninstall: $unit_path" >&2; exit 1; }
    assert_contains "systemctl --user daemon-reload" "$supervisor_log"
    assert_contains "systemctl --user enable operond.service" "$supervisor_log"
    assert_contains "systemctl --user start operond.service" "$supervisor_log"
    assert_contains "systemctl --user status --no-pager operond.service" "$supervisor_log"
    assert_contains "systemctl --user stop operond.service" "$supervisor_log"
    assert_contains "systemctl --user disable --now operond.service" "$supervisor_log"
    ;;
  Darwin*)
    write_fake_unix_supervisor "$fake_bin/launchctl"
    plist_path="$HOME/Library/LaunchAgents/dev.operon.operond.plist"
    operond service install --config "$config"
    test -f "$plist_path" || { echo "missing generated launchd plist: $plist_path" >&2; exit 1; }
    macos_operond="$RELEASE_INSTALL_PREFIX_BIN/operond"
    macos_operond_var_alias="$(macos_private_var_alias "$macos_operond")"
    macos_config="$config"
    macos_config_var_alias="$(macos_private_var_alias "$macos_config")"
    assert_contains_any \
      "$plist_path" \
      "<string>$macos_operond</string>" \
      "<string>$macos_operond_var_alias</string>"
    assert_contains_any \
      "$plist_path" \
      "<string>$macos_config</string>" \
      "<string>$macos_config_var_alias</string>"
    operond service start
    operond service status
    operond service stop
    operond service uninstall
    test ! -e "$plist_path" || { echo "launchd plist still exists after uninstall: $plist_path" >&2; exit 1; }
    assert_contains "launchctl bootstrap" "$supervisor_log"
    assert_contains "launchctl kickstart -k gui/" "$supervisor_log"
    assert_contains "launchctl print gui/" "$supervisor_log"
    assert_contains "launchctl bootout gui/" "$supervisor_log"
    ;;
  MINGW64_NT-*|MSYS_NT-*|CYGWIN_NT-*|Windows_NT-*)
    write_fake_windows_sc
    (
      cd "$fake_bin"
      run_service_commands
    )
    assert_contains "sc.exe create OperonDaemon" "$supervisor_log"
    assert_contains "operond.exe\" service run --config" "$supervisor_log"
    assert_contains "sc.exe start OperonDaemon" "$supervisor_log"
    assert_contains "sc.exe query OperonDaemon" "$supervisor_log"
    assert_contains "sc.exe stop OperonDaemon" "$supervisor_log"
    assert_contains "sc.exe delete OperonDaemon" "$supervisor_log"
    ;;
  *)
    echo "unsupported release service-management smoke platform: $(uname -s)-$(uname -m)" >&2
    exit 1
    ;;
esac

echo "release service-management smoke passed for $REPO@$TAG on $asset"
