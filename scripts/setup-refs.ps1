# scripts/setup-refs.ps1
#
# Recreates the workspace-local refs/ directory by setting up directory
# junctions to the Stakpak and Claude Code source repos and copying the
# architecture reference document.
#
# Pattern: pre-flight.md decision 2 (refs/ lives inside the workspace).
# Constitution: Article II (reference codebase discipline — every team member
# must have these locally to ground citations).
#
# Usage:
#   cd C:\path\to\terrashift
#   .\scripts\setup-refs.ps1                          # use defaults
#   .\scripts\setup-refs.ps1 -StakpakSrc C:\my\agent  # override source path
#
# Notes:
#   - Directory junctions don't require admin on Windows.
#   - Junctions are NOT committed to git (refs/ is in .gitignore) — each
#     developer runs this script on their own checkout.
#   - Re-running the script is safe: existing junctions are skipped.

[CmdletBinding()]
param(
    [string]$StakpakSrc    = "C:\Users\goda\Desktop\agent",
    [string]$ClaudeCodeSrc = "C:\Users\goda\Desktop\agent\ClaudeCode-CLI-Src",
    [string]$ArchDocSrc    = "C:\Users\goda\Desktop\agent\stakpak_arch.md"
)

$ErrorActionPreference = "Stop"

# Resolve workspace root (script lives in scripts/ at the workspace root)
$WorkspaceRoot = Split-Path -Parent $PSScriptRoot
$RefsDir       = Join-Path $WorkspaceRoot "refs"

Write-Host "[setup-refs] Workspace: $WorkspaceRoot"
Write-Host "[setup-refs] Refs dir:  $RefsDir"
Write-Host ""

# Validate source paths exist
function Assert-Path($Path, $Label) {
    if (-not (Test-Path $Path)) {
        Write-Error "[setup-refs] $Label not found at: $Path"
        Write-Error "[setup-refs] Override with -$($Label.Replace(' ', '')) <path>"
        exit 1
    }
}

Assert-Path $StakpakSrc    "StakpakSrc"
Assert-Path $ClaudeCodeSrc "ClaudeCodeSrc"
Assert-Path $ArchDocSrc    "ArchDocSrc"

# Ensure refs/ exists
if (-not (Test-Path $RefsDir)) {
    New-Item -ItemType Directory -Path $RefsDir | Out-Null
    Write-Host "[setup-refs] Created refs/"
}

# Junction: refs/stakpak -> StakpakSrc
$StakpakJunction = Join-Path $RefsDir "stakpak"
if (Test-Path $StakpakJunction) {
    Write-Host "[setup-refs] refs/stakpak already exists — skipping"
} else {
    New-Item -ItemType Junction -Path $StakpakJunction -Target $StakpakSrc | Out-Null
    Write-Host "[setup-refs] Junction created: refs/stakpak -> $StakpakSrc"
}

# Junction: refs/claude-code -> ClaudeCodeSrc
$ClaudeCodeJunction = Join-Path $RefsDir "claude-code"
if (Test-Path $ClaudeCodeJunction) {
    Write-Host "[setup-refs] refs/claude-code already exists — skipping"
} else {
    New-Item -ItemType Junction -Path $ClaudeCodeJunction -Target $ClaudeCodeSrc | Out-Null
    Write-Host "[setup-refs] Junction created: refs/claude-code -> $ClaudeCodeSrc"
}

# File copy: refs/stakpak_arch.md
$ArchDocDst = Join-Path $RefsDir "stakpak_arch.md"
Copy-Item $ArchDocSrc $ArchDocDst -Force
Write-Host "[setup-refs] Copied stakpak_arch.md ($((Get-Item $ArchDocDst).Length) bytes)"

# Verification
Write-Host ""
Write-Host "[setup-refs] === Verification ==="
Get-ChildItem $RefsDir | ForEach-Object {
    $type = if ($_.PSIsContainer) {
        if ($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint) { "junction" }
        else { "directory" }
    } else { "file" }
    Write-Host "  $($_.Name)  [$type]"
}

Write-Host ""
Write-Host "[setup-refs] Done. Test from PowerShell:"
Write-Host "  Get-ChildItem refs\stakpak\ | Select-Object -First 5"
Write-Host "  Get-ChildItem refs\claude-code\src\ | Select-Object -First 5"
Write-Host "  Get-Content refs\stakpak_arch.md -TotalCount 3"
