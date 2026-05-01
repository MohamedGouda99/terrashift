#!/usr/bin/env bash
# Terrashift pre-push git hook helper.
#
# Wire into git:
#   cp .claude/hooks/pre-push.sh .git/hooks/pre-push
#   chmod +x .git/hooks/pre-push
#
# Enforces:
#   - Article VII (CI on every push — local mirror)
#   - Article XII rule 4 (token-cost regression gate when pushing to main)

set -e

BRANCH=$(git rev-parse --abbrev-ref HEAD)

echo "[pre-push] Branch: $BRANCH"

# Skip cargo if toolchain absent (bootstrap state)
if ! command -v cargo > /dev/null 2>&1; then
    echo "[pre-push] WARN: cargo not on PATH — skipping tests"
    echo "[pre-push]       Install Rust per pre-flight.md decision 8"
else
    # Always run unit tests
    echo "[pre-push] cargo test --workspace"
    cargo test --workspace || {
        echo "[pre-push] FAIL: tests must pass before push"
        exit 1
    }

    if [ "$BRANCH" = "main" ]; then
        echo "[pre-push] On main — running eval suite (Article XII rule 4)"

        # Eval suite + token-cost regression check
        cargo test --package terrashift-eval --release || {
            echo "[pre-push] FAIL: eval suite blocked main push"
            echo "[pre-push] (Article XII rule 4 — >30% token cost increase blocks)"
            exit 1
        }

        # TODO when Claude Code exposes a CLI runner:
        #   claude run-agent eval-runner
    fi
fi

echo "[pre-push] ✓ All checks passed"
