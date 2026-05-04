// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `/scan <dir>` — read .tf files, return inventory as a Text outcome.
//!
//! Real lib call (no LLM, no network). Useful as the first interactive
//! command operators try when learning the TUI.

use super::{CommandContext, CommandOutcome, SlashCommand};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use terrashift_engine::scanner::Scanner;

pub struct Scan;

impl SlashCommand for Scan {
    fn name(&self) -> &'static str {
        "scan"
    }

    fn description(&self) -> &'static str {
        "Print resource inventory of <dir> (no LLM, read-only)"
    }

    fn run(&self, _ctx: &CommandContext, args: &str) -> CommandOutcome {
        let dir = args.trim();
        if dir.is_empty() {
            return CommandOutcome::Error("/scan requires a directory: /scan <path>".to_string());
        }

        let path = Path::new(dir);
        if !path.exists() {
            return CommandOutcome::Error(format!("path does not exist: {dir}"));
        }

        let inv = match Scanner::scan(path) {
            Ok(i) => i,
            Err(e) => return CommandOutcome::Error(format!("scan failed: {e}")),
        };

        let total_files = inv.files.len();
        let total_resources: usize = inv.files.iter().map(|f| f.resources.len()).sum();

        let mut out = String::new();
        let _ = writeln!(out, "📂 Scanning {dir} ...");
        let _ = writeln!(
            out,
            "✓ {total_files} file(s); {total_resources} resource(s)"
        );

        if total_resources > 0 {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for f in &inv.files {
                for r in &f.resources {
                    *counts.entry(r.resource_type.as_str()).or_insert(0) += 1;
                }
            }
            let _ = writeln!(out);
            let _ = writeln!(out, "Resource types:");
            for (rtype, count) in &counts {
                let _ = writeln!(out, "  {rtype}: {count}");
            }
        }

        CommandOutcome::Text(out.trim_end().to_string())
    }
}
