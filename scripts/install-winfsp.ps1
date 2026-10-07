# Install the supported WinFsp SDK from its pinned, verified official asset.
# Chocolatey's feed can report success with zero packages after a 504. Verify
# the SDK/registry explicitly instead of letting Rust fail later in build.rs.
$ErrorActionPreference = "Stop"

function Test-WinFspSdk {
    $key = Get-ItemProperty -LiteralPath "HKLM:\SOFTWARE\WOW6432Node\WinFsp" -ErrorAction SilentlyContinue
    if (-not $key -or -not $key.InstallDir) { return $false }
    $library = Join-Path $key.InstallDir "lib\winfsp-x64.lib"
    $runtime = Join-Path $key.InstallDir "bin\winfsp-x64.dll"
    if ((Test-Path -LiteralPath $library) -and (Test-Path -LiteralPath $runtime)) {
        Write-Host "WinFsp SDK verified at $($key.InstallDir)"
        return $true
    }
    return $false
}

if (Test-WinFspSdk) { exit 0 }

$installEvidence = Join-Path ([System.IO.Path]::GetTempPath()) ("operon-winfsp-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $installEvidence | Out-Null
$installer = Join-Path $installEvidence "winfsp-2.1.25156.msi"
$url = "https://github.com/winfsp/winfsp/releases/download/v2.1/winfsp-2.1.25156.msi"
$expectedHash = "073a70e00f77423e34bed98b86e600def93393ba5822204fac57a29324db9f7a"
for ($attempt = 1; $attempt -le 3; $attempt++) {
    try {
        Invoke-WebRequest -Uri $url -OutFile $installer -UseBasicParsing
        break
    } catch {
        if ($attempt -eq 3) { throw }
        Start-Sleep -Seconds 3
    }
}
if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedHash) {
    throw "WinFsp installer checksum mismatch"
}
$signature = Get-AuthenticodeSignature -LiteralPath $installer
if ($signature.Status -ne "Valid") { throw "WinFsp installer signature is $($signature.Status)" }
Write-Host "Verified official WinFsp MSI: SHA256=$expectedHash; publisher=$($signature.SignerCertificate.Subject)"
$log = Join-Path $installEvidence "install.log"
$process = Start-Process -FilePath "msiexec.exe" -ArgumentList @("/i", "`"$installer`"", "/qn", "/norestart", "ADDLOCAL=ALL", "/l*v", "`"$log`"") -Wait -PassThru
Write-Host "WinFsp installer exit=$($process.ExitCode); evidence=$installEvidence"
if ($process.ExitCode -notin @(0, 3010)) {
    Get-Content -LiteralPath $log -Tail 80
    throw "WinFsp MSI installation failed"
}
if (-not (Test-WinFspSdk)) { throw "WinFsp installation completed without the SDK/registry required by winfsp_wrs_sys" }
