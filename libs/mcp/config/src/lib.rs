// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Shared MCP config types — used by client, server, and proxy.
//!
//! Pattern: the architecture reference section 13 (config crate is the leaf for the
//! MCP triple — keeps client/server/proxy from depending on each other).

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
