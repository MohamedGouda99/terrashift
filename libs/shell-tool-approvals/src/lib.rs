//! Tree-sitter-bash command parsing + max-restrictive verdict resolution.
//!
//! Article XIII rule 6 enforcement primitive: never approve a shell
//! pipeline wholesale. The Executor (P-09b) consumes this crate to
//! gate every `terraform plan`/`apply`/`destroy` invocation.
//!
//! Pattern: stakpak_arch.md §17 (lines 1811-1886) + §30 (lines
//! 2307-2313). Source: refs/stakpak/libs/shell-tool-approvals/src/
//! (lifted with documented narrowing — Stage 1 has exact-match args
//! only; full regex/glob/cache machinery deferred to S5+).
//!
//! Constitution: Article V (security boundary), Article XIII rule 1
//! (failure-closed by default; the `clamp_failure_closed` idiom
//! ensures `ParseError` never yields `Allow`), Article XIII rule 6
//! (per-command resolution + `max()` aggregation makes wholesale
//! `terraform apply` approval a *type-system impossibility*).
//!
//! ## Public API
//!
//! ```no_run
//! use terrashift_shell_tool_approvals::{resolve, stage1_policy, Verdict};
//!
//! let verdict = resolve(
//!     "terraform apply mainplan.tfplan",
//!     &stage1_policy(),
//!     Verdict::Prompt,
//! );
//! assert_eq!(verdict, Verdict::Deny);
//! ```

pub mod matcher;
pub mod parse;
pub mod resolver;

pub use parse::{parse, ParseError, ParsedCommand, MAX_SCRIPT_DEPTH};
pub use resolver::{parse_or_fail, resolve, resolve_parsed, Policy};

use std::collections::HashMap;

/// What the gate decides. Discriminants are explicit so the derived
/// `Ord` does the right thing under `Iterator::max()` —
/// `Allow(0) < Prompt(1) < Deny(2)`. Most-restrictive wins.
///
/// Mirrors `refs/stakpak/tui/src/services/auto_approve.rs:20-29`'s
/// `AutoApprovePolicy` shape: same numeric ordering, named differently
/// because Terrashift's domain (terraform commands at the executor
/// boundary) speaks the language of Allow/Prompt/Deny rather than
/// Auto/Prompt/Never.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    Allow = 0,
    Prompt = 1,
    Deny = 2,
}

impl Default for Verdict {
    /// Article XIII rule 1: failure-closed default. Operator must
    /// explicitly opt to anything other than Prompt.
    fn default() -> Self {
        Verdict::Prompt
    }
}

/// Failure-closed clamp idiom from `refs/stakpak/tui/src/services/auto_approve.rs:486-498`.
/// Any `Allow` in the input snaps up to `Prompt`; `Deny` stays `Deny`.
/// `ParseError` paths flow through here so a parse failure CAN NEVER
/// yield `Allow` — Article IV / §30 idiom.
pub fn clamp_failure_closed(verdict: Verdict) -> Verdict {
    verdict.max(Verdict::Prompt)
}

/// The canonical Stage 1 rule map. Seeded for the Executor's actual
/// invocations (terraform, cargo, git, echo). Other commands fall
/// back to the caller's `default` verdict (typically `Prompt`).
///
/// Per reference-explorer §F: this is intentionally minimal. S5+ may
/// grow this to include cloud-CLI verbs (aws, gcloud, az) once the
/// Cred broker (P-10) wires their auth path.
pub fn stage1_policy() -> Policy {
    let mut p: HashMap<String, Verdict> = HashMap::new();

    // terraform — Article XIII rule 6: apply/destroy NEVER wholesale.
    p.insert("run_command::terraform".to_string(), Verdict::Prompt);
    p.insert("run_command::terraform::plan".to_string(), Verdict::Allow);
    p.insert(
        "run_command::terraform::validate".to_string(),
        Verdict::Allow,
    );
    p.insert("run_command::terraform::fmt".to_string(), Verdict::Allow);
    p.insert("run_command::terraform::init".to_string(), Verdict::Allow);
    p.insert("run_command::terraform::show".to_string(), Verdict::Allow);
    p.insert("run_command::terraform::output".to_string(), Verdict::Allow);
    p.insert("run_command::terraform::apply".to_string(), Verdict::Deny);
    p.insert("run_command::terraform::destroy".to_string(), Verdict::Deny);

    // cargo — common dev commands.
    p.insert("run_command::cargo".to_string(), Verdict::Prompt);
    p.insert("run_command::cargo::build".to_string(), Verdict::Allow);
    p.insert("run_command::cargo::check".to_string(), Verdict::Allow);
    p.insert("run_command::cargo::test".to_string(), Verdict::Allow);
    p.insert("run_command::cargo::fmt".to_string(), Verdict::Allow);
    p.insert("run_command::cargo::clippy".to_string(), Verdict::Allow);
    p.insert("run_command::cargo::run".to_string(), Verdict::Prompt);

    // git — read paths Allow, write paths Prompt.
    p.insert("run_command::git".to_string(), Verdict::Prompt);
    p.insert("run_command::git::status".to_string(), Verdict::Allow);
    p.insert("run_command::git::diff".to_string(), Verdict::Allow);
    p.insert("run_command::git::log".to_string(), Verdict::Allow);
    p.insert("run_command::git::show".to_string(), Verdict::Allow);
    p.insert("run_command::git::push".to_string(), Verdict::Prompt);
    p.insert("run_command::git::commit".to_string(), Verdict::Prompt);

    // echo — always safe.
    p.insert("run_command::echo".to_string(), Verdict::Allow);

    // Adversarial commands that must NEVER be auto-approved even if
    // they sneak into a pipeline. Article XIII rule 6 belt-and-braces.
    p.insert("run_command::rm".to_string(), Verdict::Deny);
    p.insert("run_command::curl".to_string(), Verdict::Prompt);
    p.insert("run_command::wget".to_string(), Verdict::Prompt);
    p.insert("run_command::nc".to_string(), Verdict::Deny);

    // `env`/`xargs` evasion (Stage 1 stopgap; security-auditor LOW #1).
    // Stakpak's parser handles `env bash -c "..."` by walking the
    // ENV_VALUED_ARGS table to find the inner script. Terrashift Stage 1
    // narrows the parser to direct `<shell> -c` only — so until S5+
    // ports the env/xargs nested-script extraction (Stakpak parse.rs
    // lines 217-268), we slam the door on env-prefixed evasion by
    // marking `env` itself Deny. The Executor (P-09b) doesn't invoke
    // `env` directly so this has zero false-positive risk for Stage 1.
    p.insert("run_command::env".to_string(), Verdict::Deny);
    p.insert("run_command::xargs".to_string(), Verdict::Deny);

    p
}
