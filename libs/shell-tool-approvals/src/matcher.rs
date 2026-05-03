//! Argument-level pattern matcher (Stage 1: exact-match only).
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md)
//! ships full regex (`re:` prefix), glob auto-detection, and a
//! `OnceLock<Mutex<PatternCache>>`. Stage 1 narrows to exact
//! string equality — the rule maps Stage 1 needs (terraform/cargo/
//! git/echo) don't yet require pattern matching.
//!
//! Stage 5+ swaps in the full the reference matcher when the rule map grows
//! to include arg patterns (e.g., `terraform::apply::-target=*`).
//!
//! Constitution: Article IV (no silent partial-match fallthrough;
//! exact-match means exact).

/// Stage 1 exact-match. Returns true iff `pattern == arg`.
///
/// The `_` prefix on the function tells reviewers this is the Stage
/// 1 narrow form; in S5+ the signature stays the same (`(pattern,
/// arg) -> bool`) but the body grows to handle `re:` prefix +
/// glob detection.
pub fn matches(pattern: &str, arg: &str) -> bool {
    pattern == arg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_works() {
        assert!(matches("apply", "apply"));
        assert!(!matches("apply", "destroy"));
        assert!(!matches("apply", "Apply")); // case-sensitive
    }

    #[test]
    fn empty_strings_match_each_other() {
        assert!(matches("", ""));
        assert!(!matches("", "x"));
        assert!(!matches("x", ""));
    }
}
