// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Persistent opt-in state for telemetry. Default: `Disabled`.
//! Stored at `~/.terrashift/telemetry.json` so a CLI run and a TUI run
//! share the same decision.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum OptIn {
    #[default]
    Disabled,
    Enabled,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Persisted {
    opt_in: OptIn,
    asked: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum OptInError {
    #[error("read {0}: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("write {0}: {1}")]
    Write(PathBuf, std::io::Error),
    #[error("parse {0}: {1}")]
    Parse(PathBuf, serde_json::Error),
    #[error("home directory not found")]
    HomeMissing,
}

fn default_path() -> Result<PathBuf, OptInError> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or(OptInError::HomeMissing)?;
    Ok(PathBuf::from(home).join(".terrashift").join("telemetry.json"))
}

pub fn current_state(path: Option<&Path>) -> Result<OptIn, OptInError> {
    let p = match path {
        Some(p) => p.to_path_buf(),
        None => default_path()?,
    };
    if !p.exists() {
        return Ok(OptIn::Disabled);
    }
    let raw = std::fs::read_to_string(&p).map_err(|e| OptInError::Read(p.clone(), e))?;
    let parsed: Persisted = serde_json::from_str(&raw).map_err(|e| OptInError::Parse(p.clone(), e))?;
    Ok(parsed.opt_in)
}

pub fn should_prompt_for_optin(path: Option<&Path>) -> Result<bool, OptInError> {
    let p = match path {
        Some(p) => p.to_path_buf(),
        None => default_path()?,
    };
    if !p.exists() {
        return Ok(true);
    }
    let raw = std::fs::read_to_string(&p).map_err(|e| OptInError::Read(p.clone(), e))?;
    let parsed: Persisted = serde_json::from_str(&raw).map_err(|e| OptInError::Parse(p.clone(), e))?;
    Ok(!parsed.asked)
}

pub fn record_choice(opt_in: OptIn, path: Option<&Path>) -> Result<(), OptInError> {
    let p = match path {
        Some(p) => p.to_path_buf(),
        None => default_path()?,
    };
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| OptInError::Write(p.clone(), e))?;
    }
    let val = Persisted { opt_in, asked: true };
    let raw = serde_json::to_string_pretty(&val).map_err(|e| {
        OptInError::Parse(p.clone(), e)
    })?;
    std::fs::write(&p, raw).map_err(|e| OptInError::Write(p, e))?;
    Ok(())
}
