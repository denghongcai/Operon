$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

$Tmp = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("operon-v014-win-" + [System.Guid]::NewGuid()))
$Workspace = Join-Path $Tmp.FullName "workspace"
$Store = Join-Path $Tmp.FullName "store.jsonl"
$Config = Join-Path $Tmp.FullName "config.yaml"
$DaemonLog = Join-Path $Tmp.FullName "daemon.log"
$DaemonErr = Join-Path $Tmp.FullName "daemon.err.log"
$MountLog = Join-Path $Tmp.FullName "mount.log"
$MountErr = Join-Path $Tmp.FullName "mount.err.log"
$Daemon = $null
$Mount = $null
$MountPoint = $null

function Stop-ChildProcess {
    param($Process)
    if ($null -ne $Process -and -not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        Wait-Process -Id $Process.Id -ErrorAction SilentlyContinue
    }
}

function Cleanup {
    Stop-ChildProcess $Mount
    Stop-ChildProcess $Daemon
    Remove-Item -Recurse -Force $Tmp.FullName -ErrorAction SilentlyContinue
}

try {
    New-Item -ItemType Directory -Path $Workspace | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $Workspace "seed.txt"), "seed")

    @"
version: 1
daemon:
  node_id: windows-live
  grpc_listen: 127.0.0.1:18842
  workspace: $($Workspace -replace '\\', '/')
  store: $($Store -replace '\\', '/')
  auth:
    token: windows-live-token
client:
  nodes:
    windows-live:
      endpoint: grpc://127.0.0.1:18842
      auth:
        token: windows-live-token
policy:
  subject: v014-windows-live-smoke
  fs:
    mounts:
      - name: workspace
        path: /
        permissions:
          read: true
          write: true
          delete: true
  exec:
    allowed_cwds:
      - /
    default_timeout_secs: 30
    max_timeout_secs: 30
    env_allowlist: []
    allowed_secrets: []
  service:
    services: []
"@ | Set-Content -Path $Config -NoNewline

    $Daemon = Start-Process -FilePath "cargo" -ArgumentList @("run", "-q", "-p", "operond", "--", "start", "--config", $Config) -RedirectStandardOutput $DaemonLog -RedirectStandardError $DaemonErr -PassThru -NoNewWindow

    $ready = $false
    for ($i = 0; $i -lt 30; $i++) {
        & cargo run -q -p operon-cli -- --config $Config node ping windows-live *> $null
        if ($LASTEXITCODE -eq 0) {
            $ready = $true
            break
        }
        Start-Sleep -Seconds 1
    }
    if (-not $ready) {
        & cargo run -q -p operon-cli -- --config $Config node ping windows-live
    }

    foreach ($letter in @("O:", "P:", "Q:", "R:")) {
        if (-not (Test-Path $letter)) {
            $MountPoint = $letter
            break
        }
    }
    if (-not $MountPoint) {
        throw "no free drive letter for WinFsp mount"
    }

    $Mount = Start-Process -FilePath "cargo" -ArgumentList @("run", "-q", "-p", "operon-cli", "--", "--config", $Config, "mount", "windows-live:/", "--to", $MountPoint) -RedirectStandardOutput $MountLog -RedirectStandardError $MountErr -PassThru -NoNewWindow

    $mounted = $false
    for ($i = 0; $i -lt 30; $i++) {
        if (Test-Path "$MountPoint\seed.txt") {
            $mounted = $true
            break
        }
        Start-Sleep -Seconds 1
    }
    if (-not $mounted) {
        Get-Content $MountLog -ErrorAction SilentlyContinue | Write-Error
        Get-Content $MountErr -ErrorAction SilentlyContinue | Write-Error
        throw "mount did not expose seed file"
    }

    if ((Get-Content "$MountPoint\seed.txt" -Raw) -ne "seed") {
        throw "seed file content mismatch"
    }

    [System.IO.File]::WriteAllText("$MountPoint\new.txt", "created through windows mount")
    & cargo run -q -p operon-cli -- --config $Config fs read windows-live:/new.txt | Set-Content (Join-Path $Tmp.FullName "new-read.txt")
    if ((Get-Content (Join-Path $Tmp.FullName "new-read.txt") -Raw).Trim() -ne "created through windows mount") {
        throw "remote read after write mismatch"
    }

    New-Item -ItemType Directory -Path "$MountPoint\dir" | Out-Null
    [System.IO.File]::WriteAllText("$MountPoint\dir\data.txt", "abcdef")
    $stream = [System.IO.File]::Open("$MountPoint\dir\data.txt", [System.IO.FileMode]::Open, [System.IO.FileAccess]::Write)
    try {
        $stream.SetLength(3)
    } finally {
        $stream.Dispose()
    }
    if ((Get-Content "$MountPoint\dir\data.txt" -Raw) -ne "abc") {
        throw "truncate through mount failed"
    }
    Move-Item "$MountPoint\dir\data.txt" "$MountPoint\dir\renamed.txt"
    & cargo run -q -p operon-cli -- --config $Config fs read windows-live:/dir/renamed.txt | Set-Content (Join-Path $Tmp.FullName "renamed-read.txt")
    if ((Get-Content (Join-Path $Tmp.FullName "renamed-read.txt") -Raw).Trim() -ne "abc") {
        throw "remote read after rename mismatch"
    }
    Remove-Item "$MountPoint\dir\renamed.txt"
    Remove-Item "$MountPoint\dir"

    Write-Host "v0.14 Windows live mount smoke passed"
} finally {
    Cleanup
}
