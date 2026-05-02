//! MCP proxy — multi-upstream multiplexer with mTLS and the redaction boundary.
//!
//! Pattern: stakpak_arch.md section 13 + section 26 (mTLS via in-process CA
//! from rcgen — keys never touch disk).
//! Constitution: Article V (THE redaction point — see Article XIII rule 5),
//! Article XIII rule 5 (don't bypass — every tool result through redact_content).

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
