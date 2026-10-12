# Installing Operon

## TypeScript SDK

```sh
npm install @cellworks/operon-sdk-js
```

The npm package includes platform-specific native bindings for optional iroh
connections; ordinary gRPC remains the default. Node.js 22 or newer is required.
From v0.16.16, GitHub Releases distribute only daemon/CLI archives and checksums;
SDK packages are distributed through npm. Older release assets remain available.

This page holds the full install path behind the README
[Quickstart](../README.md#quickstart): release archives, libc selection, live
mount prerequisites, managed daemon services, shell completions, and agent
skills.

## Install scripts

Linux and macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/denghongcai/Operon/main/scripts/install.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/denghongcai/Operon/main/scripts/install.ps1 | iex
```

Both installers resolve the latest release (or `OPERON_VERSION`), download the
archive for the current platform, verify it against the release `SHA256SUMS`,
and install `operon` and `operond`:

- [`scripts/install.sh`](../scripts/install.sh) installs into `/usr/local/bin`
  (using `sudo` only when needed), or `OPERON_INSTALL_DIR`. On macOS it also
  installs the bundled `libfuse-t.dylib` next to the binaries. Linux libc
  detection follows the [release archive](#release-archives) rules; `OPERON_RELEASE_LIBC` overrides it. Run
  it with `--dry-run` to print the selected asset without downloading.
- [`scripts/install.ps1`](../scripts/install.ps1) installs into
  `%LOCALAPPDATA%\Operon`, or `OPERON_INSTALL_DIR`, and adds that directory to
  the user `PATH`. Set `OPERON_INSTALL_DRY_RUN=1` to print the selected asset.

To pin a version:

```bash
curl -fsSL https://raw.githubusercontent.com/denghongcai/Operon/main/scripts/install.sh | OPERON_VERSION=v0.16.15 bash
```

## Manual install

Download `operon-<tag>-<platform>.tar.gz` (or `.zip` on Windows) and
`SHA256SUMS` from the [releases page](https://github.com/denghongcai/Operon/releases),
verify the checksum, extract the archive, and put `operon` and `operond` on
your `PATH`. On macOS, keep `libfuse-t.dylib` next to the binaries.

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
