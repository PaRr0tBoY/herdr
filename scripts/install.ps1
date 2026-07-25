#!/usr/bin/env pwsh
<#
.SYNOPSIS
    herdr install script (Windows)
    Downloads the latest release from PaRr0tBoY/herdr
.DESCRIPTION
    Usage: irm <raw-url> | iex
    Or:    .\install.ps1 [-InstallDir <path>]
#>
[CmdletBinding()]
param(
    [string]$InstallDir = $env:HERDR_INSTALL_DIR
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Repo = "PaRr0tBoY/herdr"
$ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"
$AssetName = "herdr-windows-x86_64.exe"

# ---- preflight ----
if ($env:OS -ne "Windows_NT") {
    Write-Error "install.ps1 is for Windows. Use install.sh on Linux or macOS."
    exit 1
}

if (-not [Environment]::Is64BitOperatingSystem) {
    Write-Error "Herdr requires 64-bit Windows."
    exit 1
}

$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($arch -eq "Arm64") {
    Write-Host "==> ARM64 detected; installing x86_64 build under emulation"
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\Herdr\bin"
}

# ---- fetch release ----
Write-Host "==> Fetching latest release from $Repo..."
try {
    $release = Invoke-RestMethod -Uri $ApiUrl -TimeoutSec 20
} catch {
    Write-Error "Can't reach GitHub API. Check your connection."
    exit 1
}

$version = $release.tag_name
if (-not $version) {
    Write-Error "Could not parse version from release."
    exit 1
}

$asset = $release.assets | Where-Object { $_.name -eq $AssetName }
if (-not $asset) {
    Write-Error "Asset '$AssetName' not found in release $version."
    exit 1
}

# ---- download ----
Write-Host "==> Downloading $version ($AssetName)..."
$tmpDir = Join-Path $env:TEMP "herdr-install-$([System.Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

try {
    $exePath = Join-Path $tmpDir "herdr.exe"

    try {
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $exePath -TimeoutSec 300
    } catch {
        Write-Error "Download failed: $_"
        exit 1
    }

    # ---- sha256 verification ----
    $shaUrl = "$($asset.browser_download_url).sha256"
    try {
        $shaPath = Join-Path $tmpDir "herdr.exe.sha256"
        Invoke-WebRequest -Uri $shaUrl -OutFile $shaPath -TimeoutSec 10 -ErrorAction Stop
        $expected = ((Get-Content $shaPath -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
        $actual = (Get-FileHash -Algorithm SHA256 $exePath).Hash.ToLowerInvariant()
        if ($expected -ne $actual) {
            Write-Error "SHA-256 mismatch. Expected $expected, got $actual."
            exit 1
        }
        Write-Host "==> Checksum verified"
    } catch [System.Net.WebException] {
        Write-Warning "No checksum file found; skipping verification"
    }

    # ---- smoke test ----
    & $exePath --version *> $null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Downloaded binary failed --version check."
        exit 1
    }

    # ---- install ----
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $targetPath = Join-Path $InstallDir "herdr.exe"
    Copy-Item -LiteralPath $exePath -Destination $targetPath -Force

    Write-Host "==> Installed to $targetPath"

    # ---- PATH ----
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($null -eq $userPath) { $userPath = "" }
    $installEntry = $InstallDir.TrimEnd('\')
    $inPath = $userPath.Split(';') | Where-Object { $_.TrimEnd('\') -ieq $installEntry }

    if (-not $inPath) {
        $newPath = "$InstallDir;$userPath".TrimEnd(';')
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        $env:Path = "$InstallDir;$env:Path"
        Write-Host "==> Added $InstallDir to user PATH"
    } else {
        Write-Host "==> $InstallDir is already on PATH"
    }

    Write-Host "Done. Herdr $version installed. Open a new terminal and run: herdr"

} finally {
    Remove-Item -LiteralPath $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
