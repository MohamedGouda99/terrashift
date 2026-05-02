# scripts/setup-fixtures.ps1
#
# Clones external test fixtures into terrashift/fixtures/ for eval + integration tests.
# Fixtures are NOT committed to the Terrashift repo — they're per-developer downloads.
#
# Why? Many useful fixtures are GPL-2.0 or other copyleft licenses. Bundling them
# into the Apache-2.0 Terrashift repo would create a license entanglement. Cloning
# them into a gitignored directory keeps Terrashift's distribution clean while
# letting tests reference real-world examples.
#
# Pattern: pre-flight.md decision 10.
# Constitution: Article II (reference codebase discipline — fixtures are reference data).
#
# Usage:
#   cd C:\path\to\terrashift
#   .\scripts\setup-fixtures.ps1
#
# Re-running is idempotent (existing fixtures get `git pull` instead of re-clone).

[CmdletBinding()]
param(
    [string]$FixturesDir = $null
)

$ErrorActionPreference = "Stop"

# Resolve workspace root (script lives in scripts/ at the workspace root)
$WorkspaceRoot = Split-Path -Parent $PSScriptRoot
if (-not $FixturesDir) { $FixturesDir = Join-Path $WorkspaceRoot "fixtures" }

Write-Host "[setup-fixtures] Workspace: $WorkspaceRoot" -ForegroundColor Cyan
Write-Host "[setup-fixtures] Fixtures dir: $FixturesDir"
Write-Host ""

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Error "[setup-fixtures] git not on PATH"
    exit 1
}

# Ensure fixtures/ exists
if (-not (Test-Path $FixturesDir)) {
    New-Item -ItemType Directory -Path $FixturesDir | Out-Null
    Write-Host "[setup-fixtures] Created fixtures/"
}

# -----------------------------------------------------------------------------
# Fixture 1: PratikMahajan/AWS-to-AZURE-Infrastructure-Migration
# License: GPL-2.0 (used as-is for testing; never bundled with Terrashift)
# Source: pre-flight.md decision 10
# -----------------------------------------------------------------------------
Write-Host ""
Write-Host "[1] PratikMahajan AWS->Azure migration..." -ForegroundColor Cyan
$dst = Join-Path $FixturesDir "aws-to-azure-real"
if (Test-Path (Join-Path $dst ".git")) {
    Write-Host "    Already cloned — pulling latest"
    git -C $dst pull --ff-only origin master 2>&1 | Select-Object -Last 3
} elseif (Test-Path $dst) {
    Write-Error "    $dst exists but is not a git repo — delete manually first"
    exit 1
} else {
    Write-Host "    Cloning..."
    git clone https://github.com/PratikMahajan/AWS-to-AZURE-Infrastructure-Migration.git $dst
}

# -----------------------------------------------------------------------------
# Verification
# -----------------------------------------------------------------------------
Write-Host ""
Write-Host "[done] === Verification ===" -ForegroundColor Green
Write-Host "Fixtures available:"
Get-ChildItem $FixturesDir -Directory | ForEach-Object {
    $tfCount = (Get-ChildItem $_.FullName -Recurse -Filter "*.tf" -ErrorAction SilentlyContinue).Count
    Write-Host "  $($_.Name)  ($tfCount .tf files)"
}
Write-Host ""
Write-Host "[done] License notice:"
Write-Host "  PratikMahajan repo is GPL-2.0. Use only as a test fixture."
Write-Host "  Do NOT copy its .tf files into the Terrashift repo source tree."
Write-Host "  Mappings derived by running Terrashift against it are first-party (Apache-2.0)."
