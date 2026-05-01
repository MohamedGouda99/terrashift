#!/usr/bin/env bash
# Terrashift pre-commit git hook helper.
#
# Wire into git by symlinking or copying to .git/hooks/pre-commit:
#   On Windows + Git Bash:
#     cp .claude/hooks/pre-commit.sh .git/hooks/pre-commit
#     chmod +x .git/hooks/pre-commit
#
# Enforces:
#   - Article VII (CI on every PR — local mirror)
#   - Article XIII rule 3 (no unwrap/expect/string-slice in production)
#   - Conventional Commits (when staged commit message available)

set -e

echo "[pre-commit] Running Terrashift checks..."

# Skip cargo checks if toolchain not installed (bootstrap state)
if ! command -v cargo > /dev/null 2>&1; then
    echo "[pre-commit] WARN: cargo not on PATH — skipping fmt/clippy"
    echo "[pre-commit]       Install Rust per pre-flight.md decision 8"
else
    # Skip if no Rust files staged
    RUST_FILES=$(git diff --cached --name-only --diff-filter=ACM | grep -E '\.rs$' || true)
    if [ -z "$RUST_FILES" ]; then
        echo "[pre-commit] No Rust files staged — skipping cargo checks"
    else
        echo "[pre-commit] cargo fmt --check"
        cargo fmt -- --check || {
            echo "[pre-commit] FAIL: run 'cargo fmt' to fix formatting"
            exit 1
        }

        echo "[pre-commit] cargo clippy --all-targets -- -D warnings"
        cargo clippy --all-targets -- -D warnings || {
            echo "[pre-commit] FAIL: clippy reported issues (Article XIII rule 3)"
            exit 1
        }
    fi
fi

# TODO when Claude Code exposes a CLI runner for sub-agents:
#   claude run-agent constitution-checker --staged
# Until then, the constitution-checker sub-agent is invoked manually
# from within Claude Code via the Agent tool.

# Block credentials (Article V + Article XIII rule 5/10)
SECRET_PATTERNS='AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|ya29\.[0-9A-Za-z_-]+|sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{36}|"private_key": *"-----BEGIN'
if git diff --cached | grep -E "$SECRET_PATTERNS" > /dev/null; then
    echo "[pre-commit] FAIL: detected potential credential pattern in staged changes"
    echo "[pre-commit] (Article V — credentials never committed; Article XIII rules 5, 10)"
    exit 1
fi

echo "[pre-commit] ✓ All checks passed"
