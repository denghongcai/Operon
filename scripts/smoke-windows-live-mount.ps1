$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

$Tmp = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("operon-live-mount-win-" + [System.Guid]::NewGuid()))
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
$OperondExe = Join-Path $Root "target\debug\operond.exe"
$OperonExe = Join-Path $Root "target\debug\operon.exe"

function Write-LogFile {
    param($Label, $Path)
    Write-Host "=== $Label ($Path) ==="
    if (Test-Path $Path) {
        $content = Get-Content $Path -Raw -ErrorAction SilentlyContinue
        if ([string]::IsNullOrWhiteSpace($content)) {
            Write-Host "<empty>"
        } else {
            Write-Host $content
        }
    } else {
        Write-Host "<missing>"
    }
}

function Invoke-DiagnosticCommand {
    param($Label, [scriptblock]$Command)
    Write-Host "=== $Label ==="
    try {
        $output = & $Command 2>&1 | Out-String
        if ([string]::IsNullOrWhiteSpace($output)) {
            Write-Host "<empty>"
        } else {
            Write-Host $output
        }
        if ($global:LASTEXITCODE -ne 0) {
            Write-Host "$Label exit code: $global:LASTEXITCODE"
            $global:LASTEXITCODE = 0
        }
    } catch {
        Write-Host "$Label diagnostic failed: $($_.Exception.Message)"
    }
}

function Dump-Diagnostics {
    Write-Host "temporary smoke directory: $($Tmp.FullName)"
    Write-Host "mount point: $MountPoint"
    if ($null -ne $Daemon) {
        Write-Host "daemon pid: $($Daemon.Id), exited: $($Daemon.HasExited), exit_code: $(if ($Daemon.HasExited) { $Daemon.ExitCode } else { '<running>' })"
    }
    if ($null -ne $Mount) {
        Write-Host "mount pid: $($Mount.Id), exited: $($Mount.HasExited), exit_code: $(if ($Mount.HasExited) { $Mount.ExitCode } else { '<running>' })"
    }
    Get-Service -Name "WinFsp*" -ErrorAction SilentlyContinue | Format-List | Out-String | Write-Host
    Get-PSDrive -PSProvider FileSystem -ErrorAction SilentlyContinue | Format-Table -AutoSize | Out-String | Write-Host
    Write-LogFile "daemon stdout" $DaemonLog
    Write-LogFile "daemon stderr" $DaemonErr
    Write-LogFile "mount stdout" $MountLog
    Write-LogFile "mount stderr" $MountErr
    Invoke-DiagnosticCommand "fsutil drives" { & cmd.exe /c "fsutil fsinfo drives" }
    Invoke-DiagnosticCommand "mountvol" { & cmd.exe /c "mountvol" }
    if ($MountPoint) {
        Invoke-DiagnosticCommand "fsutil drive type" { & cmd.exe /c "fsutil fsinfo drivetype $MountPoint\" }
        Invoke-DiagnosticCommand "fsutil volume info" { & cmd.exe /c "fsutil fsinfo volumeinfo $MountPoint\" }
        Invoke-DiagnosticCommand "cmd dir mount root" { & cmd.exe /c "dir $MountPoint\" }
        Invoke-DiagnosticCommand "powershell list mount root" { Get-ChildItem -Force "$MountPoint\" | Format-List }
    }
    Get-ChildItem -Force $Tmp.FullName -ErrorAction SilentlyContinue | Format-List | Out-String | Write-Host
}

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
  subject: live-mount-windows-live-smoke
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

    & cargo build -q -p operond -p operon-cli --locked --features operon-mount/winfsp-debug
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed"
    }

    $Daemon = Start-Process -FilePath $OperondExe -ArgumentList @("start", "--config", $Config) -RedirectStandardOutput $DaemonLog -RedirectStandardError $DaemonErr -PassThru -NoNewWindow

    $ready = $false
    for ($i = 0; $i -lt 30; $i++) {
        & $OperonExe --config $Config node ping windows-live *> $null
        if ($LASTEXITCODE -eq 0) {
            $ready = $true
            break
        }
        Start-Sleep -Seconds 1
    }
    if (-not $ready) {
        & $OperonExe --config $Config node ping windows-live
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

    $previousMountTrace = $env:OPERON_MOUNT_TRACE
    $env:OPERON_MOUNT_TRACE = "1"
    try {
        $Mount = Start-Process -FilePath $OperonExe -ArgumentList @("--config", $Config, "mount", "windows-live:/", "--to", $MountPoint) -RedirectStandardOutput $MountLog -RedirectStandardError $MountErr -PassThru -NoNewWindow
    } finally {
        if ($null -eq $previousMountTrace) {
            Remove-Item Env:\OPERON_MOUNT_TRACE -ErrorAction SilentlyContinue
        } else {
            $env:OPERON_MOUNT_TRACE = $previousMountTrace
        }
    }

    $mounted = $false
    for ($i = 0; $i -lt 20; $i++) {
        if ($Mount.HasExited) {
            Dump-Diagnostics
            throw "mount process exited before exposing seed file"
        }
        if (Test-Path "$MountPoint\seed.txt") {
            $mounted = $true
            break
        }
        Start-Sleep -Seconds 1
    }
    if (-not $mounted) {
        Dump-Diagnostics
        throw "mount did not expose seed file"
    }

    if ((Get-Content "$MountPoint\seed.txt" -Raw) -ne "seed") {
        throw "seed file content mismatch"
    }

    [System.IO.File]::WriteAllText("$MountPoint\new.txt", "created through windows mount")
    & $OperonExe --config $Config fs read windows-live:/new.txt | Set-Content (Join-Path $Tmp.FullName "new-read.txt")
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
    & $OperonExe --config $Config fs read windows-live:/dir/renamed.txt | Set-Content (Join-Path $Tmp.FullName "renamed-read.txt")
    if ((Get-Content (Join-Path $Tmp.FullName "renamed-read.txt") -Raw).Trim() -ne "abc") {
        throw "remote read after rename mismatch"
    }
    Remove-Item "$MountPoint\dir\renamed.txt"
    Remove-Item "$MountPoint\dir"

    Write-Host "Windows live mount smoke passed"
} catch {
    Dump-Diagnostics
    throw
} finally {
    Cleanup
}
