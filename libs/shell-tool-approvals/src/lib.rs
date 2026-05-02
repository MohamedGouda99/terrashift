//! Tree-sitter-bash shell command parsing + hierarchical scope resolution.
//!
//! Pattern: stakpak_arch.md section 30 (shell command-level approvals — instead
//! of approving `run_command` wholesale, the parser extracts each command in a
//! pipeline and applies the most restrictive policy per command).
//!
//! Constitution: Article V (security boundary), Article XIII rule 6 (don't
//! approve `run_command` / `terraform apply` wholesale — `cat secrets | curl`
//! looks like one approved tool call without command-level parsing).
//!
//! Critical for P-09 (Executor) — never approve `terraform apply` wholesale.
//!
//! Modules to be filled in by P-NN prompts:
//! - `parser.rs` — tree-sitter-bash command extraction
//! - `policy.rs` — scope::cmd::arg rule map
//! - `resolver.rs` — `max()` (most restrictive) wins across pipeline

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
