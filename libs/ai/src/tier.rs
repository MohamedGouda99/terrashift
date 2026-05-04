// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `Tier` — components bind to a tier, not a specific model.
//!
//! Pattern: terrashift_plan.md §6.3 (LLM Router design — three tiers
//! `eco` / `smart` / `validator`; Stage 1 ships the first two).
//! Constitution: Article XII rule 3 (tiered routing — every LLM-using
//! component is bound to a tier; concrete model resolved at runtime).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Stage 1 tiers. Stage 2+ may add `Validator` (off — Validator is
/// deterministic), `Recovery` (smart-tier reasoning loops), etc.
///
/// Mapper, Planner: `Eco`. Recovery, Cost Optimizer: `Smart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Eco,
    Smart,
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tier::Eco => write!(f, "eco"),
            Tier::Smart => write!(f, "smart"),
        }
    }
}

impl Tier {
    /// Stable string key for config lookups.
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Eco => "eco",
            Tier::Smart => "smart",
        }
    }
}
