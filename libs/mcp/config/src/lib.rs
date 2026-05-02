//! Shared MCP config types — used by client, server, and proxy.
//!
//! Pattern: stakpak_arch.md section 13 (config crate is the leaf for the
//! MCP triple — keeps client/server/proxy from depending on each other).

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
