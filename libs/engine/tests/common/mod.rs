// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Shared test helpers for `libs/engine` integration tests.
//!
//! Cargo convention: `tests/common/mod.rs` is the entry point for code
//! shared across `tests/*.rs` integration test binaries. Each test file
//! that needs these helpers does `mod common;` then `common::seed_fixtures::...`.
//!
//! Constitution: Article VI (`libs/knowledge/seed/` is the schema source
//! of truth — tests read from it; never embed required-attribute lists
//! inline). See `feedback_no_hardcoding_use_seed.md` in user memory.

#![allow(dead_code)] // Different test binaries use different subsets.

pub mod seed_fixtures;
