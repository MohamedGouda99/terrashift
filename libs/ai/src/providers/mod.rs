// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Concrete provider implementations.
//!
//! Pattern: the architecture reference §39 row 1 / the reference codebase (see ATTRIBUTIONS.md)
//! Each provider lives in its own subdirectory with the canonical
//! `{convert, mod, provider, stream, types}.rs` shape (the reference parity).
//!
//! Stage 1 active:
//! - `openai_compat` — Groq, OpenAI, custom OpenAI-shape gateways
//!   (Vodafone-internal, Together, Anyscale, etc.)
//!
//! Stage 2+ stubs (each returns `AiError::UnsupportedProviderType`
//! until its session lands; same pattern as libs/creds/{aws,gcp,azure}):
//! - `anthropic` — S2+
//! - `gemini`    — S2+
//! - `bedrock`   — S3+

pub mod anthropic;
pub mod bedrock;
pub mod gemini;
pub mod openai_compat;
