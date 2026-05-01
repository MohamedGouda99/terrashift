# scripts/setup-refs.ps1
#
# Sets up the workspace-local refs/ directory:
#   - refs/stakpak/      = fresh git clone of stakpak/agent (clean upstream view)
#   - refs/claude-code/  = directory junction to local Claude Code source
#   - refs/stakpak_arch.md = file copy of the architecture reference document
#
# Pattern: pre-flight.md decision 2 (refs/ lives inside the workspace).
# Constitution: Article II (reference codebase discipline).
#
# Usage:
#   cd C:\path\to\terrashift
#   .\scripts\setup-refs.ps1                                # use defaults
#   .\scripts\setup-refs.ps1 -ClaudeCodeSrc D:\my\cc-src   # override Claude Code source
#   .\scripts\setup-refs.ps1 -ArchDocSrc D:\arch.md        # override arch doc source
#   .\scripts\setup-refs.ps1 -ShallowClone                 # shallow clone Stakpak
#                                                            (faster, loses branch refs)
#
# Why clone instead of junction for stakpak?
#   The user's local stakpak working copy may contain unrelated additions
#   (terrashift_v5/, .claude/, .specify/, etc.). A fresh clone gives a clean
#   upstream-only mirror. Cloning is one-time (~50-200 MB); the junction was
#   leaking everything in the working tree.
#
# Why junction for claude-code?
#   The local ClaudeCode-CLI-Src/ folder is already isolated to Claude Code
#   source only. Junction-ing avoids a duplicate clone with no benefit.
#
# Re-running the script is safe:
#   - If refs/stakpak/ already exists as a clone, we git-pull instead of re-clone
#   - If refs/claude-code/ junction exists, skipped
#   - The arch doc is always re-copied (cheap, ensures freshness)

[CmdletBinding()]
param(
    [string]$StakpakRepo   = "https://github.com/stakpak/agent.git",
    [string]$ClaudeCodeSrc = "C:\Users\goda\Desktop\agent\ClaudeCode-CLI-Src",
    [string]$ArchDocSrc    = "C:\Users\goda\Desktop\agent\stakpak_arch.md",
    [switch]$ShallowClone
)

$ErrorActionPreference = "Stop"

# Resolve workspace root (script lives in scripts/ at the workspace root)
$WorkspaceRoot = Split-Path -Parent $PSScriptRoot
$RefsDir       = Join-Path $WorkspaceRoot "refs"

Write-Host "[setup-refs] Workspace: $WorkspaceRoot"
Write-Host "[setup-refs] Refs dir:  $RefsDir"
Write-Host ""

# Validate sources we need on disk (Claude Code + arch doc)
function Assert-Path($Path, $Label) {
    if (-not (Test-Path $Path)) {
        Write-Error "[setup-refs] $Label not found at: $Path"
        Write-Error "[setup-refs] Override with -$($Label.Replace(' ', '')) <path>"
        exit 1
    }
}

Assert-Path $ClaudeCodeSrc "ClaudeCodeSrc"
Assert-Path $ArchDocSrc    "ArchDocSrc"

# Validate git is installed
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Error "[setup-refs] git not on PATH — required for cloning Stakpak"
    exit 1
}

# Ensure refs/ exists
if (-not (Test-Path $RefsDir)) {
    New-Item -ItemType Directory -Path $RefsDir | Out-Null
    Write-Host "[setup-refs] Created refs/"
}

# Clone (or update) refs/stakpak/
$StakpakClone = Join-Path $RefsDir "stakpak"
if (Test-Path (Join-Path $StakpakClone ".git")) {
    Write-Host "[setup-refs] refs/stakpak/ exists as clone — pulling latest main"
    git -C $StakpakClone fetch --all --prune
    git -C $StakpakClone pull --ff-only origin main
} elseif (Test-Path $StakpakClone) {
    # Exists but isn't a git repo — likely a stale junction or partial clone
    Write-Error "[setup-refs] refs/stakpak/ exists but is not a git repo"
    Write-Error "[setup-refs] Delete it manually first, then re-run:"
    Write-Error "[setup-refs]   [System.IO.Directory]::Delete('$StakpakClone', `$false)  # if junction"
    Write-Error "[setup-refs]   Remove-Item -Recurse -Force '$StakpakClone'             # if directory"
    exit 1
} else {
    Write-Host "[setup-refs] Cloning $StakpakRepo into refs/stakpak/"
    if ($ShallowClone) {
        Write-Host "[setup-refs] (--depth 1 — branch references will be unavailable)"
        git clone --depth 1 $StakpakRepo $StakpakClone
    } else {
        git clone $StakpakRepo $StakpakClone
    }
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
        if ($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
            "junction"
        } elseif (Test-Path (Join-Path $_.FullName ".git")) {
            "git clone"
        } else {
            "directory"
        }
    } else { "file" }
    Write-Host "  $($_.Name)  [$type]"
}

Write-Host ""
Write-Host "[setup-refs] Done. Test from PowerShell:"
Write-Host "  Get-ChildItem refs\stakpak\ | Select-Object -First 5"
Write-Host "  git -C refs\stakpak log -1 --oneline"
Write-Host "  Get-ChildItem refs\claude-code\src\ | Select-Object -First 5"
Write-Host "  Get-Content refs\stakpak_arch.md -TotalCount 3"
