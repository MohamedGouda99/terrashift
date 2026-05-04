// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Executor failure modes.
//!
//! Pattern: matches `libs/engine/src/{scanner,validator,generator}/errors.rs`
//! shape — one named variant per concern. Constitution: Article IV.

use std::path::PathBuf;
use terrashift_shell_tool_approvals::Verdict;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutorError {
    /// `pre_apply_check` resolved to `Verdict::Deny` for one of the
    /// terraform invocations. The Executor refuses to run the
    /// command — Article XIII rule 6.
    #[error("approval gate denied: command='{command}' verdict={verdict:?} (Article XIII rule 6)")]
    DeniedByApprovalGate { command: String, verdict: Verdict },

    /// `pre_apply_check` resolved to `Verdict::Prompt` and the
    /// Executor was configured for non-interactive mode (no human
    /// can confirm). Stage 1: this short-circuits to a refusal so
    /// CI runs that need explicit approval don't silently fall
    /// through to subprocess execution.
    #[error("approval gate requires Prompt but executor is non-interactive: command='{command}'")]
    PromptRequiredButNonInteractive { command: String },

    /// Stage 1 placeholder — actual terraform subprocess
    /// invocation is gated on Docker + cloud creds. The Executor
    /// constructs the command, checks the gate, would emit audit,
    /// and returns this. S5 close replaces with real subprocess.
    #[error("not implemented yet: '{which}' wires real terraform subprocess (unblocks in {session}; needs Docker + cloud test creds)")]
    NotImplementedYet {
        which: &'static str,
        session: &'static str,
    },

    /// Working directory creation / .tf file copy / output write
    /// failed.
    #[error("filesystem I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Wrapped audit-store error. Boxed for clippy `result_large_err`.
    #[error("audit append failed: {0}")]
    Audit(Box<terrashift_audit::AuditError>),
}
