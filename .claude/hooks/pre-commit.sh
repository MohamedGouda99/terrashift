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

# Block credentials in PRODUCTION code only (Article V + Article XIII rule 5/10).
#
# Test files under tests/ AND #[cfg(test)] inline module fixtures are EXCLUDED
# because:
#   1. Scrubber + audit tests need fake-secret-shaped fixtures to verify the
#      RUNTIME scrubber catches them (libs/audit's whole point — see
#      libs/audit/tests/audit_chain_test.rs::raw_aws_key_in_payload_panics).
#   2. The constitutional defence is the runtime scrubber (Article XIII rule 5),
#      which fires on every audit write. The commit-time hook is a belt-and-
#      suspenders check for production code, not test fixtures.
#
# Excluding tests/* keeps both invariants honest: production code stays clean,
# test fixtures stay realistic enough to actually test what they claim to test.
SECRET_PATTERNS='AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|ya29\.[0-9A-Za-z_-]+|sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{36}|"private_key": *"-----BEGIN'
# Files excluded because their CONTENT is the regex patterns themselves
# (not actual leaked secrets). Adding new entries here requires Article XI.
EXCLUDE='(/tests/|_test\.rs$|/fixtures/|scrubber\.rs$|pre-commit\.sh$|pre-commit\.original$|TERRASHIFT_MAPPING\.md$|SESSION_PLAN\.md$|CONSTITUTION\.md$|terrashift_prompts\.md$|terrashift_plan\.md$|specs/.*\.md$)'
NON_TEST_FILES=$(git diff --cached --name-only --diff-filter=ACM | grep -vE "$EXCLUDE" || true)
if [ -n "$NON_TEST_FILES" ]; then
    if echo "$NON_TEST_FILES" | xargs -I {} git diff --cached -- {} | grep -E "$SECRET_PATTERNS" > /dev/null 2>&1; then
        echo "[pre-commit] FAIL: detected potential credential pattern in staged production code"
        echo "[pre-commit] (Article V — credentials never committed; Article XIII rules 5, 10)"
        echo "[pre-commit] Excluded paths: tests/, _test.rs, fixtures/, scrubber.rs, hook itself, doc files."
        exit 1
    fi
fi

echo "[pre-commit] ✓ All checks passed"
