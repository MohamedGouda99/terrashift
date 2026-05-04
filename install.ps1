# Terrashift — one-line installer for Windows.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -Command "iwr -useb https://raw.githubusercontent.com/MohamedGouda99/terrashift/main/install.ps1 | iex"
#
# Or, with options:
#   $env:TERRASHIFT_VERSION = "v0.1.0"
#   $env:TERRASHIFT_PREFIX  = "C:\tools\terrashift"
#   iwr -useb https://raw.githubusercontent.com/MohamedGouda99/terrashift/main/install.ps1 | iex
#
# Note: Today's release matrix ships Linux + macOS binaries. Windows
# users currently need to build from source until cross-compilation is
# added to the release workflow. This script will detect that case and
# guide you to the source-build path.
#
# Copyright (c) 2026 Mohamed Gouda. All rights reserved.
# Licensed under the Terrashift Source-Available License v1.0 — see
# https://github.com/MohamedGouda99/terrashift/blob/main/LICENSE.

#Requires -Version 5.1

[CmdletBinding()]
param(
    [string]$Version = $env:TERRASHIFT_VERSION,
    [string]$Prefix  = $env:TERRASHIFT_PREFIX,
    [string]$Repo    = $(if ($env:TERRASHIFT_REPO) { $env:TERRASHIFT_REPO } else { "MohamedGouda99/terrashift" })
)

$ErrorActionPreference = "Stop"

# ─────────────────────────────────────────────────────────────────────
# Defaults
# ─────────────────────────────────────────────────────────────────────
if (-not $Version) { $Version = "latest" }
if (-not $Prefix)  { $Prefix  = Join-Path $env:LOCALAPPDATA "Programs\Terrashift\bin" }
$BinaryName = "terrashift.exe"

# ─────────────────────────────────────────────────────────────────────
# Output helpers
# ─────────────────────────────────────────────────────────────────────
function Say  ($msg) { Write-Host "[install] $msg" }
function OK   ($msg) { Write-Host "[install] $msg" -ForegroundColor Green }
function Warn ($msg) { Write-Host "[install] $msg" -ForegroundColor Yellow }
function Die  ($msg) {
    Write-Host "[install] ERROR: $msg" -ForegroundColor Red
    exit 1
}

# ─────────────────────────────────────────────────────────────────────
# Platform detection
# ─────────────────────────────────────────────────────────────────────
# PowerShell 5.1 doesn't have $IsWindows; check $env:OS instead.
if ($env:OS -ne "Windows_NT") {
    Die "this script targets Windows. On Linux/macOS use install.sh instead."
}

# Map architecture
$arch = $env:PROCESSOR_ARCHITECTURE
switch ($arch) {
    "AMD64" { $ArchTag = "x86_64" }
    "ARM64" { $ArchTag = "aarch64" }
    default { Die "unsupported architecture: $arch" }
}

# ─────────────────────────────────────────────────────────────────────
# Today's release matrix is Linux + macOS only. Detect this and
# fail early with a constructive message so the user isn't left
# guessing why a 404 happened.
# ─────────────────────────────────────────────────────────────────────
Say "platform: windows-$ArchTag"
Warn @"
Windows binaries are not yet published in the release pipeline.
Today's release matrix: linux-x86_64, darwin-aarch64.

For now, build from source:

    git clone https://github.com/$Repo.git
    cd terrashift
    cargo build --release --bin terrashift
    # binary lands at: target\release\terrashift.exe

Add the binary's directory to your PATH, or copy the .exe somewhere
already on PATH.

To request Windows binary builds, open an issue:
    https://github.com/$Repo/issues

"@

# Stop here on Windows until the release pipeline ships windows-msvc
# artifacts. The rest of this script is wired and ready for that day —
# just remove the exit below and add the corresponding artifact name to
# the release matrix.
exit 0

# ─────────────────────────────────────────────────────────────────────
# (Below this line: ready for the day Windows artifacts ship.)
# ─────────────────────────────────────────────────────────────────────

# Artifact name once Windows is in the release matrix.
# Suggested name when added to .github/workflows/release.yml:
#   terrashift-windows-x86_64.zip
$Artifact = "terrashift-windows-$ArchTag"

# Resolve version
if ($Version -eq "latest") {
    Say "resolving latest release from GitHub API..."
    $url = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $rel = Invoke-RestMethod -UseBasicParsing -Uri $url -Headers @{ "User-Agent" = "terrashift-installer" }
        $Version = $rel.tag_name
    } catch {
        Die "could not resolve latest release: $($_.Exception.Message)"
    }
}
Say "installing version: $Version"

# Build URLs
$base = "https://github.com/$Repo/releases/download/$Version"
$zipName     = "$Artifact.zip"
$shaName     = "$Artifact.zip.sha256"
$zipUrl      = "$base/$zipName"
$shaUrl      = "$base/$shaName"

$workdir = New-Item -ItemType Directory `
    -Path (Join-Path $env:TEMP ("terrashift-install-" + [System.Guid]::NewGuid().ToString("N").Substring(0,8))) `
    -Force
$zipPath = Join-Path $workdir $zipName
$shaPath = Join-Path $workdir $shaName

try {
    Say "downloading $zipUrl"
    Invoke-WebRequest -UseBasicParsing -Uri $zipUrl -OutFile $zipPath

    # Checksum verification (optional — older releases may not ship one)
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $shaUrl -OutFile $shaPath -ErrorAction Stop
        Say "verifying SHA256"
        $expected = (Get-Content $shaPath -First 1).Split()[0]
        $actual   = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($expected.ToLowerInvariant() -ne $actual) {
            Die "SHA256 mismatch. Expected: $expected, got: $actual"
        }
        OK "checksum verified"
    } catch {
        Warn "checksum file not found — proceeding without verification."
    }

    # Extract
    Say "extracting"
    Expand-Archive -Path $zipPath -DestinationPath $workdir -Force

    $sourceBin = Join-Path $workdir $BinaryName
    if (-not (Test-Path $sourceBin)) {
        Die "expected $BinaryName not found in archive"
    }

    # Install
    if (-not (Test-Path $Prefix)) {
        New-Item -ItemType Directory -Path $Prefix -Force | Out-Null
    }
    $installPath = Join-Path $Prefix $BinaryName
    Move-Item -Path $sourceBin -Destination $installPath -Force
    OK "installed to $installPath"

    # PATH guidance
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$Prefix*") {
        Warn "$Prefix is NOT on your User PATH."
        Write-Host ""
        Write-Host "        To add it for your user (no admin required), run in PowerShell:" -ForegroundColor Yellow
        Write-Host ""
        Write-Host "            [Environment]::SetEnvironmentVariable('Path', '$Prefix;' + [Environment]::GetEnvironmentVariable('Path', 'User'), 'User')" -ForegroundColor Cyan
        Write-Host ""
        Write-Host "        Then close and reopen your terminal." -ForegroundColor Yellow
        Write-Host ""
    } else {
        OK "$Prefix is on your PATH — open a new terminal and run ``terrashift``."
    }

    OK "done. Try: terrashift --help"
} finally {
    Remove-Item -Path $workdir -Recurse -Force -ErrorAction SilentlyContinue
}
