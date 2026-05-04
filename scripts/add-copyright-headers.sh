#!/usr/bin/env bash
# Add copyright headers to every .rs file in cli/, tui/, libs/.
# Idempotent: skips files that already start with a "// Copyright" line.
#
# Run from the workspace root:
#   bash scripts/add-copyright-headers.sh
#
# After running, `cargo fmt --all` and `cargo build` should still pass.
# The header sits ABOVE any `//!` inner doc comments or `#![...]` crate-
# level attributes — both are valid Rust positions.

set -euo pipefail

HEADER='// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.
'

added=0
skipped=0

while IFS= read -r f; do
    # Skip if already has a copyright line in the first 5 lines
    if head -n 5 "$f" | grep -q "^// Copyright"; then
        skipped=$((skipped + 1))
        continue
    fi
    # Prepend header + blank line
    {
        printf '%s\n' "$HEADER"
        cat "$f"
    } > "$f.tmp"
    mv "$f.tmp" "$f"
    added=$((added + 1))
done < <(find cli tui libs -name "*.rs" -type f)

echo "[copyright-headers] added: $added, skipped: $skipped"
