//! Tree-sitter-bash command extraction.
//!
//! Pattern: the architecture reference §17 (lines 1811-1886). Source:
//! the reference codebase (see ATTRIBUTIONS.md)
//! (verbatim DFS walker + parser reuse + nested -c handling).
//!
//! Stage 1 mirrors the reference's structure exactly. The only narrowing:
//! we don't yet implement the `ENV_VALUED_ARGS` / `XARGS_VALUED_FLAGS`
//! tables that the reference uses to handle env-var prefixes and xargs
//! flag-arg pairing — Stage 1 Executor only invokes simple
//! `terraform <subcommand>` and `cargo <subcommand>` shapes.
//!
//! Constitution: Article IV (loud errors — `ParseError` variants
//! named), Article XIII rule 3 (no panics in production).

use thiserror::Error;
use tree_sitter::{Node, Parser};

/// Maximum recursion for nested `bash -c "..."` scripts. Stage 1
/// matches the reference's `parse.rs:57` cap of 5; deeper nesting is
/// adversarial input.
pub const MAX_SCRIPT_DEPTH: usize = 5;

/// Shells that take a `-c` script argument we should recurse into.
/// Stage 1 mirrors the reference's set; Terrashift Executor doesn't
/// invoke these directly but the resolver uses the same list to
/// catch evasion attempts (`bash -c "rm -rf /"`).
const SHELLS: &[&str] = &["sh", "bash", "zsh", "ksh", "dash"];

/// One command extracted from the AST. Mirrors
/// `the reference codebase (see ATTRIBUTIONS.md)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub name: Option<String>,
    pub args: Vec<String>,
    /// Byte offset of the command's `name` node in the original input.
    /// Diagnostic only; not used for resolution.
    pub offset: usize,
}

/// Parse failures. Two variants only — tree-sitter-bash is
/// permissive, so most "malformed" inputs simply produce no
/// commands. The variants here cover:
/// 1. The bash language couldn't be loaded (process-level failure).
/// 2. Nested `bash -c "..."` recursion exceeded `MAX_SCRIPT_DEPTH`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("tree-sitter-bash language unavailable")]
    ParserUnavailable,
    #[error("nested -c script recursion exceeded {0} levels (adversarial input?)")]
    NestingLimitExceeded(usize),
}

/// Walk the AST and extract every command node. DFS with explicit
/// stack (matches the reference's pattern at `parse.rs:84-106`). Reverse-
/// child push so DFS yields commands in source order.
///
/// Returns `Ok(vec![])` for empty / comment-only / whitespace-only
/// inputs — that's the contract: "no commands found, default verdict
/// applies."
pub fn parse(input: &str) -> Result<Vec<ParsedCommand>, ParseError> {
    let mut all_commands: Vec<ParsedCommand> = Vec::new();
    let mut scripts: Vec<(String, usize)> = vec![(input.to_string(), 0)];
    let mut parser = Parser::new();

    if parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .is_err()
    {
        return Err(ParseError::ParserUnavailable);
    }

    while let Some((script, depth)) = scripts.pop() {
        if depth > MAX_SCRIPT_DEPTH {
            return Err(ParseError::NestingLimitExceeded(MAX_SCRIPT_DEPTH));
        }
        parser.reset();
        let Some(tree) = parser.parse(&script, None) else {
            continue;
        };
        let mut stack: Vec<Node> = vec![tree.root_node()];

        while let Some(node) = stack.pop() {
            if node.kind() == "command" {
                if let Some(parsed) = extract_command(node, script.as_bytes()) {
                    // Detect nested shell -c "<inner>" — push the inner
                    // script onto the stack so we recurse into it.
                    //
                    // **Divergence from the reference (security-auditor LOW #2):**
                    // the reference's parser pushes BOTH the outer `bash` command
                    // AND the inner script onto the result list (its
                    // `parse.rs:90-96` always pushes `cmd`). Terrashift
                    // narrows: when nested extraction succeeds, we elide
                    // the outer shell command. Reasoning: the outer shell
                    // is plumbing — the inner script carries the actual
                    // intent — and eliding it prevents a future
                    // permissive `bash → Allow` rule from accidentally
                    // greenlighting the wrapper. Both behaviors yield the
                    // same Deny verdict for `bash -c "rm -rf /"` because
                    // `rm` is Deny; this divergence is about WHICH
                    // commands feed the `max()`, not the final verdict.
                    if let Some(inner) = nested_dash_c_script(&parsed) {
                        scripts.push((inner, depth + 1));
                    } else {
                        all_commands.push(parsed);
                    }
                }
            }
            // Push children in REVERSE so DFS visits them in source order.
            // Mirrors the reference's `parse.rs:99-101` pattern.
            // tree-sitter 0.26: `child_count()` returns `usize`,
            // `child(i)` takes `u32`. AST child counts are always
            // tiny so a `try_from` overflow is unreachable; fall
            // back to 0 (no iteration) to stay Article XIII rule 3
            // clean.
            let count: u32 = u32::try_from(node.child_count()).unwrap_or(0);
            for i in (0..count).rev() {
                if let Some(child) = node.child(i) {
                    stack.push(child);
                }
            }
        }
    }

    Ok(all_commands)
}

/// Pull a single `command` node into `(name, args, offset)`.
fn extract_command(node: Node, src: &[u8]) -> Option<ParsedCommand> {
    let mut name: Option<String> = None;
    let mut args: Vec<String> = Vec::new();
    let mut offset: usize = node.start_byte();

    let count: u32 = u32::try_from(node.child_count()).unwrap_or(0);
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        let kind = child.kind();
        // tree-sitter-bash classifies the head of a `command` as
        // "command_name" and subsequent positional args as just
        // children of the command node (often "word" or quoted
        // strings).
        let text = src
            .get(child.byte_range())
            .and_then(|b| std::str::from_utf8(b).ok())
            .map(|s| s.to_string())
            .unwrap_or_default();

        if text.is_empty() {
            continue;
        }
        match kind {
            "command_name" => {
                name = Some(text);
                offset = child.start_byte();
            }
            "word" | "string" | "raw_string" | "concatenation" | "number" => {
                args.push(unquote(&text));
            }
            _ => {} // skip variables/heredocs/redirects for Stage 1
        }
    }

    if name.is_none() && args.is_empty() {
        return None;
    }
    Some(ParsedCommand { name, args, offset })
}

/// If the parsed command is `<shell> -c "<inner_script>"`, return the
/// inner script (unquoted) so the caller can push it onto the parse
/// stack for recursion. Otherwise None.
fn nested_dash_c_script(parsed: &ParsedCommand) -> Option<String> {
    let name = parsed.name.as_deref()?;
    if !SHELLS.contains(&name) {
        return None;
    }
    // Find the `-c` flag and take the next positional arg as the script.
    let dash_c_idx = parsed.args.iter().position(|a| a == "-c")?;
    parsed.args.get(dash_c_idx + 1).cloned()
}

/// Strip leading/trailing matching quotes from a token. Tree-sitter
/// gives us the raw byte range including quotes; we want the inner
/// content for resolver matching.
fn unquote(s: &str) -> String {
    if s.len() >= 2 {
        let first = s.chars().next();
        let last = s.chars().last();
        if let (Some(f), Some(l)) = (first, last) {
            if (f == '"' && l == '"') || (f == '\'' && l == '\'') {
                // Strip via char-boundary-safe slice (Article XIII rule 3).
                let len = s.len();
                if let Some(stripped) = s.get(1..len.saturating_sub(1)) {
                    return stripped.to_string();
                }
            }
        }
    }
    s.to_string()
}
