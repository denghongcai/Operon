$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

# Only suspend the daemon created below, never an operator-selected process.
if (-not ("OperonReadLifecycle.Native" -as [type])) {
    Add-Type @"
using System;
using System.Runtime.InteropServices;
namespace OperonReadLifecycle {
    public static class Native {
        [DllImport("ntdll.dll")]
        public static extern int NtSuspendProcess(IntPtr process);
        [DllImport("ntdll.dll")]
        public static extern int NtResumeProcess(IntPtr process);
    }
}
"@
}
$waitSeconds = 30
if ($env:OPERON_SMOKE_SHUTDOWN_TIMEOUT_SECS) {
    $waitSeconds = [int]$env:OPERON_SMOKE_SHUTDOWN_TIMEOUT_SECS
}
if ($waitSeconds -le 0 -or $waitSeconds -gt 3600) { throw "Invalid smoke shutdown timeout" }
$waitMs = $waitSeconds * 1000
$tmp = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("operon-read-lifecycle-" + [guid]::NewGuid()))
$workspace = New-Item -ItemType Directory -Path (Join-Path $tmp.FullName "workspace")
$token = Join-Path $tmp.FullName "token"
$config = Join-Path $tmp.FullName "config.yaml"
$daemon = $null
$owner = $null
$stream = $null
$paused = $false
try {
    & cargo build -q --locked -p operond -p operon-mount --bins --example session-lifecycle --features operon-mount/winfsp-debug
    if ($LASTEXITCODE -ne 0) { throw "Lifecycle helper build failed" }
    [IO.File]::WriteAllText($token, "windows-lifecycle-test-token")
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $listener.Stop()
    @"
version: 1
daemon:
  node_id: lifecycle
  grpc_listen: 127.0.0.1:$port
  workspace: $($workspace.FullName -replace '\\', '/')
  advertise_lan: false
  auth: {token: windows-lifecycle-test-token}
client:
  nodes:
    lifecycle:
      endpoint: grpc://127.0.0.1:$port
      auth: {token: windows-lifecycle-test-token}
policy:
  subject: lifecycle-test
  fs:
    mounts:
      - name: workspace
        path: /
        permissions: {read: true, write: true, delete: true}
  exec:
    allowed_cwds: []
    default_timeout_secs: 30
    max_timeout_secs: 30
    env_allowlist: []
    allowed_secrets: []
  service: {services: []}
"@ | Set-Content $config -NoNewline
    $daemon = Start-Process (Join-Path $Root "target\debug\operond.exe") -ArgumentList @("start", "--config", $config) -RedirectStandardOutput (Join-Path $tmp.FullName "daemon.log") -RedirectStandardError (Join-Path $tmp.FullName "daemon.err") -PassThru -NoNewWindow
    $ready = $false
    for ($i = 0; $i -lt $waitSeconds; $i++) {
        & (Join-Path $Root "target\debug\operon.exe") --config $config node ping lifecycle *> $null
        if ($LASTEXITCODE -eq 0) { $ready = $true; break }
        if ($daemon.HasExited) { throw "Test daemon exited during startup" }
        Start-Sleep -Seconds 1
    }
    if (-not $ready) { throw "Test daemon readiness failed" }
    $drive = $null
    foreach ($letter in @("O:", "P:", "Q:", "R:")) {
        if (-not (Test-Path $letter)) { $drive = $letter; break }
    }
    if (-not $drive) { throw "No free drive for lifecycle test" }
    foreach ($action in @("drop", "unmount")) {
        $payload = New-Object byte[] (1024 * 1024)
        for ($i = 0; $i -lt $payload.Length; $i++) { $payload[$i] = 123 }
        [IO.File]::WriteAllBytes((Join-Path $workspace.FullName "pending-$action"), $payload)
        $info = [Diagnostics.ProcessStartInfo]::new()
        $info.FileName = Join-Path $Root "target\debug\examples\session-lifecycle.exe"
        $info.Arguments = '"grpc://127.0.0.1:{0}" "{1}" "{2}" auto' -f $port, $token, $drive
        $info.UseShellExecute = $false
        $info.RedirectStandardInput = $true
        $info.RedirectStandardOutput = $true
        $info.RedirectStandardError = $true
        $owner = [Diagnostics.Process]::new()
        $owner.StartInfo = $info
        if (-not $owner.Start()) { throw "Helper did not start" }
        $errors = $owner.StandardError.ReadToEndAsync()
        $readyLine = $owner.StandardOutput.ReadLineAsync()
        if (-not $readyLine.Wait($waitMs) -or $readyLine.Result -ne "ready") { throw "Helper readiness failed" }
        $output = $owner.StandardOutput.ReadToEndAsync()
        $stream = [IO.FileStream]::new("$drive\pending-$action", [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite, 4096, ([IO.FileOptions]::Asynchronous -bor [IO.FileOptions]::RandomAccess))
        $status = [OperonReadLifecycle.Native]::NtSuspendProcess($daemon.Handle)
        if ($status -lt 0) { throw "Could not suspend owned test daemon: $status" }
        $paused = $true
        $buffer = New-Object byte[] 4096
        $read = $stream.ReadAsync($buffer, 0, $buffer.Length)
        Start-Sleep -Milliseconds 750
        if ($read.IsCompleted) { throw "Read did not remain in flight against suspended daemon" }
        $owner.StandardInput.WriteLine($action)
        $owner.StandardInput.Flush()
        if (-not $owner.WaitForExit($waitMs)) { throw "$action did not cancel deadline-zero callback" }
        if ($owner.ExitCode -ne 0 -or -not $output.Result.Contains("joined")) { throw "Helper cleanup failed: $($errors.Result)" }
        try { $null = $read.Wait($waitMs) } catch {
            if (-not $read.IsFaulted) { throw }
        }
        if (-not $read.IsFaulted) { throw "Unmounted pending read did not fail" }
        if (Test-Path $drive) { throw "Drive survived session cleanup" }
        $status = [OperonReadLifecycle.Native]::NtResumeProcess($daemon.Handle)
        if ($status -lt 0) { throw "Could not resume owned test daemon: $status" }
        $paused = $false
        $stream.Dispose()
        $stream = $null
        $owner.Dispose()
        $owner = $null
        Write-Host "Windows $action cancelled deadline-zero in-flight read and detached drive"
    }
} catch {
    Get-ChildItem $tmp.FullName -File | ForEach-Object { Write-Host $_.Name; Get-Content $_.FullName -ErrorAction SilentlyContinue }
    throw
} finally {
    if ($paused -and $null -ne $daemon -and -not $daemon.HasExited) {
        $null = [OperonReadLifecycle.Native]::NtResumeProcess($daemon.Handle)
    }
    if ($null -ne $owner -and -not $owner.HasExited) { $owner.Kill(); $null = $owner.WaitForExit($waitMs) }
    if ($null -ne $stream) { $stream.Dispose() }
    if ($null -ne $daemon -and -not $daemon.HasExited) { Stop-Process -Id $daemon.Id -Force; $null = $daemon.WaitForExit($waitMs) }
    Remove-Item -Recurse -Force $tmp.FullName
}
