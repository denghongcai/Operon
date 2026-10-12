# Install the operon CLI and operond daemon from a public GitHub Release.
#
#   irm https://raw.githubusercontent.com/denghongcai/Operon/main/scripts/install.ps1 | iex
#
# Environment:
#   OPERON_VERSION        release tag to install (default: latest release)
#   OPERON_INSTALL_DIR    target directory (default: %LOCALAPPDATA%\Operon)
#   OPERON_INSTALL_DRY_RUN=1  print the resolved release asset without downloading it
#
# The install directory is added to the user PATH when it is not already there.

$ErrorActionPreference = "Stop"

function Install-Operon {
    $Repo = "denghongcai/Operon"

    $Version = $env:OPERON_VERSION
    if (-not $Version) {
        $Version = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest").tag_name
        if (-not $Version) { throw "failed to resolve the latest release; set OPERON_VERSION" }
    }
    if ($Version -notmatch '^v\d+\.\d+(\.\d+)?(-[A-Za-z0-9.-]+)?$') { throw "invalid release tag: $Version" }

    $Name = "operon-$Version-windows-x86_64"
    $InstallDir = if ($env:OPERON_INSTALL_DIR) { $env:OPERON_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Operon" }

    if ($env:OPERON_INSTALL_DRY_RUN -eq "1") {
        Write-Output "version=$Version"
        Write-Output "asset=$Name.zip"
        Write-Output "install_dir=$InstallDir"
        return
    }

    $Base = "https://github.com/$Repo/releases/download/$Version"
    $Work = Join-Path ([System.IO.Path]::GetTempPath()) ("operon-install-" + [System.Guid]::NewGuid())
    New-Item -ItemType Directory -Force -Path $Work | Out-Null
    try {
        $Archive = Join-Path $Work "$Name.zip"
        Write-Output "Downloading $Name.zip"
        Invoke-WebRequest "$Base/$Name.zip" -OutFile $Archive -UseBasicParsing
        Invoke-WebRequest "$Base/SHA256SUMS" -OutFile (Join-Path $Work "SHA256SUMS") -UseBasicParsing

        $Expected = $null
        foreach ($Line in Get-Content (Join-Path $Work "SHA256SUMS")) {
            $Parts = $Line -split '\s+', 2
            if ($Parts.Count -eq 2 -and $Parts[1].TrimStart('*') -eq "$Name.zip") { $Expected = $Parts[0] }
        }
        if (-not $Expected) { throw "$Name.zip is missing from SHA256SUMS" }
        $Actual = (Get-FileHash -Algorithm SHA256 $Archive).Hash
        if ($Actual -ne $Expected.ToUpperInvariant()) { throw "checksum mismatch for $Name.zip" }

        Expand-Archive -Path $Archive -DestinationPath $Work -Force
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        Copy-Item (Join-Path $Work "$Name\operon.exe") $InstallDir -Force
        Copy-Item (Join-Path $Work "$Name\operond.exe") $InstallDir -Force
    }
    finally {
        Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
    }

    Write-Output "Installed operon and operond $Version to $InstallDir"
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not (($UserPath -split ';') -contains $InstallDir)) {
        $NewPath = if ($UserPath) { "$UserPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        $env:Path = "$env:Path;$InstallDir"
        Write-Output "Added $InstallDir to the user PATH; open a new terminal to pick it up."
    }
    Write-Output "Live mount also needs WinFsp; check with: operon doctor --mount-runtime"
}

Install-Operon
