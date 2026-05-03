//! Per-command verdict resolver + max-restrictive aggregator.
//!
//! Pattern: stakpak_arch.md §17 + §30. Source:
//! refs/stakpak/libs/shell-tool-approvals/src/resolver.rs:32-42
//! (per-command find_map + outer .max() aggregation, lifted with
//! generics dropped — Stage 1 uses concrete `Verdict` instead of
//! `T: Clone + Ord`).
//!
//! Constitution: Article XIII rule 6 (max-restrictive across every
//! command in the pipeline; one Deny anywhere = Deny everywhere).

use crate::parse::{parse, ParseError, ParsedCommand};
use crate::Verdict;
use std::collections::HashMap;

/// `HashMap<scope_key, Verdict>` where scope_key is `::`-delimited.
/// Examples (mirrors Stakpak's pattern at `resolver.rs:198-216`):
///
/// - `"run_command::terraform::apply" → Verdict::Deny`
/// - `"run_command::terraform::plan"  → Verdict::Allow`
/// - `"run_command::echo"             → Verdict::Allow`
/// - `"run_command"                   → Verdict::Prompt` (scope-only fallback)
pub type Policy = HashMap<String, Verdict>;

/// Top-level resolver. Parses `input` then aggregates per-command
/// verdicts via `Iterator::max()`. Uses the supplied `default` when
/// no commands are present (empty input, comment-only).
///
/// On `ParseError`, returns the failure-closed clamp via
/// `crate::clamp_failure_closed(default)` — never `Allow`.
pub fn resolve(input: &str, policy: &Policy, default: Verdict) -> Verdict {
    let parsed = match parse(input) {
        Ok(cmds) => cmds,
        Err(_) => return crate::clamp_failure_closed(default),
    };

    if parsed.is_empty() {
        return default;
    }

    parsed
        .iter()
        .map(|cmd| resolve_command(cmd, policy, default))
        .max()
        .unwrap_or(default)
}

/// Resolve a single command's verdict. Walks the `(name, arg0, arg1,...)`
/// scope chain from most-specific to least-specific. Returns the
/// `default` when nothing matches (Article IV: explicit fallback,
/// not a panic).
fn resolve_command(cmd: &ParsedCommand, policy: &Policy, default: Verdict) -> Verdict {
    let Some(name) = cmd.name.as_deref() else {
        return default;
    };

    // Build the scope chain. Stage 1 uses positional args only:
    //   run_command::terraform::apply       (most specific)
    //   run_command::terraform              (scope-only fallback)
    //   run_command                         (root scope)
    //
    // Mirror of refs/stakpak/libs/shell-tool-approvals/src/resolver.rs:32-66
    // narrowed to exact-match (no regex/glob arg patterns until S5+).
    let mut keys: Vec<String> = Vec::with_capacity(cmd.args.len() + 2);

    // Most specific first: run_command::name::arg0::arg1::...
    let mut deepest = format!("run_command::{}", name);
    keys.push(deepest.clone());
    for arg in &cmd.args {
        // Skip flags (start with -) for scope keying. Stage 1
        // policies look like `terraform::apply` not
        // `terraform::apply::-auto-approve` — flags are positional
        // arguments to the *subcommand*, not subscopes.
        if arg.starts_with('-') {
            continue;
        }
        deepest = format!("{}::{}", deepest, arg);
        keys.push(deepest.clone());
    }
    // Reverse so we check most-specific first.
    keys.reverse();
    keys.push("run_command".to_string()); // root fallback last

    for k in &keys {
        if let Some(v) = policy.get(k) {
            return *v;
        }
    }
    default
}

/// Convenience for already-parsed commands. Used by tests and a
/// future Executor that may want to log the parsed structure before
/// resolution.
#[allow(dead_code)]
pub fn resolve_parsed(parsed: &[ParsedCommand], policy: &Policy, default: Verdict) -> Verdict {
    if parsed.is_empty() {
        return default;
    }
    parsed
        .iter()
        .map(|cmd| resolve_command(cmd, policy, default))
        .max()
        .unwrap_or(default)
}

/// Surface tree-sitter errors to the caller. Used by the Executor
/// when it wants to log parse failures separately from resolving
/// the failure-closed verdict.
pub fn parse_or_fail(input: &str) -> Result<Vec<ParsedCommand>, ParseError> {
    parse(input)
}
