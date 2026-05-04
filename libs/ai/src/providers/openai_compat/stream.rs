// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Streaming responses — S5+ stub.
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md)
//! Stage 1's `Provider::complete` is single-shot only; streaming is
//! purely a UX concern (TUI rendering as the LLM streams). When the
//! TUI runtime ships in S5+ (per TERRASHIFT_MAPPING.md §F1 plan-mode
//! lifecycle), this module fills in.
//!
//! Keep this file in place even though it's empty so the canonical
//! the reference shape (`{convert,mod,provider,stream,types}.rs`) is
//! visually present — the structure communicates intent to readers.
