# Installing Operon

This page holds the full install path behind the README
[Quickstart](../README.md#quickstart): release archives, libc selection, live
mount prerequisites, managed daemon services, shell completions, and agent
skills.

## Release archives

Linux and macOS release archives use
`.tar.gz`; Windows release archives use `.zip`. Every archive contains the
`operon` CLI and the `operond` daemon.

GNU/glibc Linux archives target glibc 2.31 or newer, such as Ubuntu 20.04+.
These archives remain unsupported on Alpine. v0.16.12 adds static musl x86_64/arm64
archives validated on Alpine 3.22/3.23, including real FUSE and OpenRC; v0.16.11
and older releases remain GNU-only. No ARMv7 musl archive is provided.
Use `OPERON_RELEASE_LIBC=gnu|musl` to override libc selection explicitly; unknown
or ambiguous environments must not silently choose an incompatible archive.
Alpine needs `bash`, `curl`, `tar`, and `fuse3` plus `/dev/fuse` for live mount.
For system services see [Alpine/OpenRC guidance](quality/alpine-openrc.md).

Live mount support is available through Linux FUSE, macOS FUSE-T, and Windows
WinFsp; hosts must have the corresponding platform runtime installed before
`operon mount` can start a filesystem session. Run
`operon doctor --mount-runtime` to check this before mounting. See
[platform support](platforms.md) for details.

## Linux and macOS

```bash
VERSION="${OPERON_VERSION:-$(curl -fsSL https://api.github.com/repos/denghongcai/Operon/releases/latest | sed -n 's/.*"tag_name": "\(v[^"]*\)".*/\1/p')}"
test -n "$VERSION" || { echo "failed to resolve latest Operon release" >&2; exit 1; }
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ARCH=linux-x86_64 ;;
  Linux-aarch64|Linux-arm64) ARCH=linux-arm64 ;;
  Linux-armv7l|Linux-armv7*) ARCH=linux-armv7 ;;
  Darwin-x86_64) ARCH=macos-x86_64 ;;
  Darwin-arm64) ARCH=macos-aarch64 ;;
  *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

if [[ "$ARCH" == linux-* ]]; then
  LIBC="${OPERON_RELEASE_LIBC:-auto}"
  if [[ "$LIBC" == auto ]]; then
    GNU=false; MUSL=false
    GNU_VERSION="$(getconf GNU_LIBC_VERSION 2>/dev/null || true)"
    LDD_VERSION="$(LC_ALL=C ldd --version 2>&1 || true)"
    [[ "$GNU_VERSION" == glibc\ * ]] && GNU=true
    [[ "$LDD_VERSION" == *GLIBC* || "$LDD_VERSION" == *'GNU libc'* || "$LDD_VERSION" == *'GNU C Library'* ]] && GNU=true
    [[ "$LDD_VERSION" == *musl* || -f /etc/alpine-release ]] && MUSL=true
    case "$GNU-$MUSL" in
      true-false) LIBC=gnu ;;
      false-true) LIBC=musl ;;
      *) echo 'unknown/ambiguous libc; set OPERON_RELEASE_LIBC=gnu or musl' >&2; exit 1 ;;
    esac
  fi
  case "$LIBC" in
    gnu) ;;
    musl)
      [[ "$ARCH" != linux-armv7 ]] || { echo 'ARMv7 musl is unsupported' >&2; exit 1; }
      ARCH="linux-musl-${ARCH#linux-}"
      ;;
    *) echo 'OPERON_RELEASE_LIBC must be auto, gnu or musl' >&2; exit 1 ;;
  esac
fi

curl -fL "https://github.com/denghongcai/Operon/releases/download/${VERSION}/operon-${VERSION}-${ARCH}.tar.gz" -o /tmp/operon.tar.gz
tar -xzf /tmp/operon.tar.gz -C /tmp
sudo install "/tmp/operon-${VERSION}-${ARCH}/operon" /usr/local/bin/operon
sudo install "/tmp/operon-${VERSION}-${ARCH}/operond" /usr/local/bin/operond
if [[ -f "/tmp/operon-${VERSION}-${ARCH}/libfuse-t.dylib" ]]; then
  sudo install -m 0644 "/tmp/operon-${VERSION}-${ARCH}/libfuse-t.dylib" /usr/local/bin/libfuse-t.dylib
fi
```

## Windows

In PowerShell:

```powershell
$Version = if ($env:OPERON_VERSION) { $env:OPERON_VERSION } else { (Invoke-RestMethod https://api.github.com/repos/denghongcai/Operon/releases/latest).tag_name }
$Arch = "windows-x86_64"
$Archive = "$env:TEMP\operon.zip"
$InstallRoot = "$env:LOCALAPPDATA\Operon"
Invoke-WebRequest "https://github.com/denghongcai/Operon/releases/download/$Version/operon-$Version-$Arch.zip" -OutFile $Archive
Expand-Archive -Path $Archive -DestinationPath $env:TEMP -Force
New-Item -ItemType Directory -Force -Path $InstallRoot | Out-Null
Copy-Item "$env:TEMP\operon-$Version-$Arch\operon.exe" $InstallRoot -Force
Copy-Item "$env:TEMP\operon-$Version-$Arch\operond.exe" $InstallRoot -Force
```

## Managed daemon service

`operond start` is the foreground runtime command and does not have a
`--background` mode. For a supervised daemon, install a platform-native service
that runs the same foreground entrypoint:

```bash
operond service install --config "$HOME/.operon/config.yaml"
operond service start
operond service status
```

`operond service stop` and `operond service uninstall` control the same
supervisor entry. The supervisor is user-level systemd on Linux, a launchd user
agent on macOS, and the Windows Service Control Manager on Windows. On Windows,
`operond service install --config <path>` registers a real Windows Service
entrypoint and may require an elevated shell depending on local service-control
policy.

## Shell completions

```bash
mkdir -p ~/.local/share/bash-completion/completions
operon completion bash > ~/.local/share/bash-completion/completions/operon

mkdir -p ~/.zfunc
operon completion zsh > ~/.zfunc/_operon
```

## Agent skills

Agent skills require Node.js 18 or newer, npm/npx, and git:

```bash
npx -y skills add https://github.com/denghongcai/Operon --list
npx -y skills add https://github.com/denghongcai/Operon --skill '*' --agent codex --yes
```

This uses the [Vercel Skills CLI](https://github.com/vercel-labs/skills) to
install Operon's repo-local skills for agents such as Codex, Claude Code,
Cursor, and other supported coding agents. Replace `codex` with your target
agent, or omit `--agent codex --yes` to choose interactively.

## Verifying a release install

Release maintainers verify the post-download install path with the manual
`Verify Release Install Usability` workflow; see
[release install usability](quality/release-install-usability.md) for the
scripts and their `--dry-run` mode.
