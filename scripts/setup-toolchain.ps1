# scripts/setup-toolchain.ps1
#
# One-command Rust toolchain setup for Windows. Installs:
#   1. LLVM-MinGW UCRT (clang + lld + libunwind + compiler-rt) into ~/llvm-mingw
#   2. Rust gnullvm toolchain (1.94.1-x86_64-pc-windows-gnullvm)
#   3. Sets the workspace override so cargo uses the matching pair
#
# Why gnullvm + LLVM-MinGW (not gnu + GCC, not msvc + VS Build Tools)?
#   - msvc requires Visual Studio Build Tools (~6 GB, admin install)
#   - gnu requires real MinGW-w64 GCC; rustup's gnu toolchain expects libgcc
#     and dies with "unable to find library -lgcc_eh" against LLVM-MinGW
#   - gnullvm is built specifically for the LLVM-MinGW stack: clang/lld/
#     compiler-rt — and works without admin
#
# Pattern: pre-flight.md decision 8.
# Constitution: not constitutional (host-specific tooling).
#
# Usage:
#   cd C:\path\to\terrashift
#   .\scripts\setup-toolchain.ps1
#
# Re-running is idempotent — existing installs are detected and skipped.

[CmdletBinding()]
param(
    [string]$LlvmMingwDir = "$env:USERPROFILE\llvm-mingw",
    [string]$RustToolchain = "1.94.1-x86_64-pc-windows-gnullvm"
)

$ErrorActionPreference = "Stop"

# Resolve workspace root (script lives in scripts/ at the workspace root)
$WorkspaceRoot = Split-Path -Parent $PSScriptRoot

Write-Host "[setup-toolchain] Workspace: $WorkspaceRoot" -ForegroundColor Cyan
Write-Host ""

# -----------------------------------------------------------------------------
# Step 1: Verify Rust is installed
# -----------------------------------------------------------------------------
Write-Host "[1/4] Checking rustup..." -ForegroundColor Cyan
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
    Write-Error "[setup-toolchain] rustup not on PATH."
    Write-Error "[setup-toolchain] Install Rust first: https://rustup.rs/"
    Write-Error "[setup-toolchain]   Then re-run this script."
    exit 1
}
Write-Host "[1/4] rustup: $(rustup --version)"

# -----------------------------------------------------------------------------
# Step 2: Install LLVM-MinGW (provides clang, lld, dlltool, compiler-rt)
# -----------------------------------------------------------------------------
Write-Host ""
Write-Host "[2/4] Checking LLVM-MinGW..." -ForegroundColor Cyan
if (Test-Path "$LlvmMingwDir\bin\clang.exe") {
    Write-Host "[2/4] LLVM-MinGW already installed at $LlvmMingwDir"
} else {
    Write-Host "[2/4] Installing LLVM-MinGW (~180 MB download)..."

    Write-Host "      Querying latest release from GitHub..."
    $rel = Invoke-RestMethod "https://api.github.com/repos/mstorsjo/llvm-mingw/releases/latest" -UseBasicParsing
    $asset = $rel.assets | Where-Object { $_.name -like "*ucrt-x86_64.zip" } | Select-Object -First 1
    if (-not $asset) {
        Write-Error "      No ucrt-x86_64.zip in latest release"
        exit 1
    }
    Write-Host "      Latest: $($rel.tag_name) — $($asset.name) ($([math]::Round($asset.size/1MB,1)) MB)"

    $zipPath = "$env:TEMP\llvm-mingw-ucrt.zip"
    Write-Host "      Downloading..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath -UseBasicParsing

    Write-Host "      Extracting to $LlvmMingwDir..."
    $tempExtract = "$env:TEMP\llvm-mingw-extract"
    if (Test-Path $tempExtract) { Remove-Item -Recurse -Force $tempExtract }
    Expand-Archive -Path $zipPath -DestinationPath $tempExtract -Force
    $innerDir = (Get-ChildItem $tempExtract -Directory | Select-Object -First 1).FullName
    Move-Item $innerDir $LlvmMingwDir
    Remove-Item -Recurse -Force $tempExtract
    Remove-Item $zipPath

    Write-Host "[2/4] Installed: $LlvmMingwDir"
}

# Add to user PATH (persistent for future shells)
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$LlvmMingwDir\bin*") {
    Write-Host "[2/4] Adding $LlvmMingwDir\bin to user PATH (persistent)..."
    [Environment]::SetEnvironmentVariable("Path", "$LlvmMingwDir\bin;$userPath", "User")
}
# Also make it available in current session
$env:Path = "$LlvmMingwDir\bin;$env:Path"

# -----------------------------------------------------------------------------
# Step 3: Install Rust gnullvm toolchain
# -----------------------------------------------------------------------------
Write-Host ""
Write-Host "[3/4] Checking Rust toolchain $RustToolchain..." -ForegroundColor Cyan
$installed = (rustup toolchain list 2>&1 | Out-String) -split "`n" | Where-Object { $_ -like "*$RustToolchain*" }
if ($installed) {
    Write-Host "[3/4] $RustToolchain already installed"
} else {
    Write-Host "[3/4] Installing $RustToolchain (~200 MB)..."
    rustup toolchain install $RustToolchain
    Write-Host "[3/4] Installed."
}

Write-Host "[3/4] Ensuring rustfmt + clippy components..."
rustup component add rustfmt --toolchain $RustToolchain
rustup component add clippy --toolchain $RustToolchain

# -----------------------------------------------------------------------------
# Step 4: Set workspace override to gnullvm
# -----------------------------------------------------------------------------
Write-Host ""
Write-Host "[4/4] Setting workspace override to $RustToolchain..." -ForegroundColor Cyan
Set-Location $WorkspaceRoot
rustup override set $RustToolchain
Write-Host "[4/4] Active toolchain: $(rustup show active-toolchain)"

# -----------------------------------------------------------------------------
# Verification
# -----------------------------------------------------------------------------
Write-Host ""
Write-Host "[done] === Verification ===" -ForegroundColor Green
Write-Host "  rustc:    $(rustc --version)"
Write-Host "  cargo:    $(cargo --version)"
Write-Host "  clang:    $(clang --version | Select-Object -First 1)"
Write-Host "  dlltool:  $((Get-Command dlltool).Source)"
Write-Host ""
Write-Host "[done] You can now run:"
Write-Host "  cargo check --all-targets" -ForegroundColor Yellow
Write-Host ""
Write-Host "[done] First run downloads ~250 dependencies and takes 5-15 minutes."
Write-Host "       Subsequent runs are incremental and complete in seconds."
