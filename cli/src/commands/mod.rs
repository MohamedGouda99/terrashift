// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! CLI subcommand implementations. Each module is a thin glue layer
//! over the corresponding lib crate(s) — the constitution's "thin
//! CLI, fat libs" rule.

pub mod migrate;
pub mod scan;
pub mod schemas;
pub mod util;
