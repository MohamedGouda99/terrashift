// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Telemetry recorder. **No-op until Stage 6**.
//!
//! Today: validates the input shape and logs at trace level. The
//! `telemetry-net` feature flag will gate the eventual POST to a
//! receiver backend; for now the flag is wired but the impl is still
//! a no-op.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub properties: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("event name empty")]
    EmptyName,
}

pub fn record_event(opt_in: super::optin::OptIn, ev: Event) -> Result<(), TelemetryError> {
    if ev.name.is_empty() {
        return Err(TelemetryError::EmptyName);
    }
    if matches!(opt_in, super::optin::OptIn::Disabled) {
        return Ok(());
    }

    #[cfg(feature = "telemetry-net")]
    {
        tracing::trace!(event = ?ev, "telemetry recorded (Stage 6 stub)");
    }

    #[cfg(not(feature = "telemetry-net"))]
    {
        tracing::trace!(event = ?ev, "telemetry recorded (no-op)");
    }

    Ok(())
}
