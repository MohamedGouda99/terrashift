// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift migrate` — run the Scanner → Mapper → Validator → Generator pipeline.
//!
//! r07-mvp-closure (this PR): wired Validator into the pipeline. The
//! prior "Stage 2 narrowing" deferred Validator until real
//! Terraform-registry pulls were cached in `KnowledgeService`. PR #6
//! (feat/schema-source-migration) satisfied that precondition by
//! shipping the bundled flat-per-version seed populated via
//! `cargo xtask capture-schemas`. Article III gate is now load-bearing
//! on the migrate path: Mapper → Validator → Generator. Per-resource
//! Validator errors land in the "skipped (gaps)" section of the
//! migration summary.
//!
//! Executor integration lands at S5-close once Docker + cloud creds are wired.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use terrashift_agent_core::PassthroughCompactionEngine;
use terrashift_ai::{JsonAgentLlmClient, Tier};
use terrashift_engine::generator::Generator;
use terrashift_engine::mapper::{Mapper, MapperCache, PassthroughContextReducer};
use terrashift_engine::recovery::{run_recovery, RecoveryConfig, RecoveryOutcome};
use terrashift_engine::scanner::Scanner;
use terrashift_engine::validator::{ValidationError, Validator};
use tokio_util::sync::CancellationToken;

use super::util::{build_knowledge_service, build_llm_client, load_profile, resolve_profile_path};

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Source directory (root of the Terraform tree to migrate).
    #[arg(long)]
    pub source: PathBuf,

    /// Output directory for emitted target HCL. Defaults to a fresh
    /// temp dir; the path is printed at the end.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Source provider key (e.g. "aws", "google", "azurerm").
    #[arg(long = "from")]
    pub from_provider: String,

    /// Target provider key (e.g. "azurerm", "aws", "google").
    #[arg(long = "to")]
    pub to_provider: String,

    /// Plan-only: scan + map + print proposed mappings, skip Generator.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// Tier to use for Mapper LLM calls (eco | smart). Default: eco.
    #[arg(long, default_value = "eco")]
    pub tier: String,
}

pub async fn run(args: Args, profile_path: Option<PathBuf>) -> Result<()> {
    // ─── 1. Load profile + build LLM client ──────────────────────────
    let profile_path = resolve_profile_path(profile_path)?;
    println!("📝 Profile: {}", profile_path.display());
    let profile = load_profile(&profile_path)?;

    let tier = parse_tier(&args.tier)?;
    let llm = build_llm_client(&profile, tier)?;
    println!("🤖 LLM tier: {tier:?} (model resolved from profile)\n");

    // ─── 2. Build KnowledgeService (loads seed) ──────────────────────
    // Wrap in Arc since both Mapper (via &) and Validator (via Arc clone)
    // consume it. Article III gate is now on the migrate path.
    let knowledge = Arc::new(build_knowledge_service().await?);

    // ─── 3. Scan source ──────────────────────────────────────────────
    println!("📂 Scanning {} ...", args.source.display());
    let inv =
        Scanner::scan(&args.source).with_context(|| format!("scan {}", args.source.display()))?;
    let total_resources: usize = inv.files.iter().map(|f| f.resources.len()).sum();
    println!(
        "✓ Scanned {} file(s); {} resource(s)\n",
        inv.files.len(),
        total_resources
    );
    if total_resources == 0 {
        println!("(nothing to migrate — no resources found in source)");
        return Ok(());
    }

    // ─── 4. Run Mapper (real LLM round-trip) ─────────────────────────
    println!(
        "🧠 Mapping {} → {} via real LLM (this calls the model; takes a few seconds) ...",
        args.from_provider, args.to_provider
    );
    let mapper = Mapper::new(&args.from_provider, &args.to_provider);
    let reducer = PassthroughContextReducer;
    let mut cache = MapperCache::new();

    let started = std::time::Instant::now();
    let mut plan = mapper
        .map(&inv, &knowledge, &llm, &reducer, &mut cache)
        .await
        .context("Mapper LLM round-trip")?;
    let elapsed = started.elapsed();

    // ─── 4b. Validator + Recovery agent (Article III gate + S10 self-healing) ─
    // r07-mvp-closure: Validator on the migrate path (Article III).
    // S10:           Recovery agent loops on Validator failures, asking the
    //                LLM to propose fixes via `set_attribute` /
    //                `change_target_type` / `remove_resource` tool calls.
    // Both are no-ops when target schema isn't cached (graceful degradation).
    let target_version = knowledge
        .schema_store
        .list_versions(&args.to_provider)
        .await
        .ok()
        .and_then(|v| v.into_iter().next());

    let validator_errors_by_addr: HashMap<String, Vec<ValidationError>> = match &target_version {
        Some(ver) => {
            let validator = Validator::new(Arc::clone(&knowledge));
            // 4b.i — Initial Validator pass.
            let initial_report = match validator.validate(&plan, ver).await {
                Ok(r) => r,
                Err(e) => {
                    println!(
                        "⚠ Validator infrastructure error (proceeding without Article III gate): {e}"
                    );
                    return Ok(());
                }
            };

            if !initial_report.errors.is_empty() {
                // 4b.ii — Errors present; invoke Recovery agent (S10).
                println!(
                    "🛠️  Validator found {} gap(s); invoking Recovery agent ...",
                    initial_report.errors.len()
                );
                let cancel = CancellationToken::new();
                // Wrap the existing RealClient in an Arc<dyn LlmClient> so
                // JsonAgentLlmClient can adapt it to the agent kernel's
                // AgentLlmClient trait. The Arc holds the same client used
                // by the Mapper above; no new connection is opened.
                let llm_arc: Arc<dyn terrashift_ai::LlmClient> = Arc::new(llm);
                let agent_llm = JsonAgentLlmClient::new(llm_arc, tier);
                let compactor = PassthroughCompactionEngine;
                let hooks: Vec<Box<dyn terrashift_agent_core::AgentHook>> = Vec::new();
                let cfg = RecoveryConfig::default();

                match run_recovery(
                    &cfg,
                    plan.clone(),
                    &validator,
                    ver,
                    &agent_llm,
                    &reducer,
                    &compactor,
                    &hooks,
                    &cancel,
                )
                .await
                {
                    Ok((updated_plan, outcome)) => {
                        plan = updated_plan;
                        match outcome {
                            RecoveryOutcome::Success {
                                iterations,
                                fixes_applied,
                            } => {
                                println!(
                                    "✓ Recovery agent succeeded ({iterations} iter(s), {fixes_applied} fix(es) applied)"
                                );
                            }
                            RecoveryOutcome::MaxIterationsReached {
                                iterations,
                                unresolved_errors,
                            } => {
                                println!(
                                    "⚠ Recovery agent hit max_iterations ({iterations}); {} error(s) unresolved",
                                    unresolved_errors.len()
                                );
                            }
                            RecoveryOutcome::AgentGaveUp {
                                iterations,
                                unresolved_errors,
                            } => {
                                println!(
                                    "⚠ Recovery agent gave up after {iterations} iteration(s); {} error(s) unresolved",
                                    unresolved_errors.len()
                                );
                            }
                            RecoveryOutcome::WaitingForApproval { iterations, .. } => {
                                println!(
                                    "⚠ Recovery agent paused at approval gate after {iterations} iteration(s) — Stage 2 narrowing"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!("⚠ Recovery agent error: {e}");
                    }
                }
            }

            // 4b.iii — Final Validator pass (after Recovery, if it ran).
            match validator.validate(&plan, ver).await {
                Ok(report) => {
                    let mut by_addr: HashMap<String, Vec<ValidationError>> = HashMap::new();
                    for err in report.errors {
                        by_addr
                            .entry(addr_of_validation_error(&err).to_string())
                            .or_default()
                            .push(err);
                    }
                    by_addr
                }
                Err(e) => {
                    println!("⚠ Validator infrastructure error on final pass: {e}");
                    HashMap::new()
                }
            }
        }
        None => {
            println!(
                "⚠ No cached schema for target provider '{}' — Validator + Recovery skipped. Run `terrashift schema update --provider {} --version <X>` to populate.",
                args.to_provider, args.to_provider
            );
            HashMap::new()
        }
    };

    println!(
        "✓ Mapper returned {} target resource(s) in {:.1}s\n",
        plan.resources.len(),
        elapsed.as_secs_f64()
    );

    // Print proposed mappings.
    println!("--- Proposed mappings ---");
    for r in &plan.resources {
        println!(
            "  {:40} → {:50} ({})",
            r.source_addr, r.target_addr, r.target_type
        );
    }
    println!();

    if args.dry_run {
        println!("(dry-run; skipping Generator. Re-run without --dry-run to emit .tf files.)");
        return Ok(());
    }

    // ─── 5. Generator ───────────────────────────────────────────────
    // r07-mvp-closure / FR-5: default-output is now `<source>-<target>/`
    // next to the source, not a tempdir that disappears. Convention-over-
    // configuration: matches the standard a user expects from a migration
    // tool. Explicit `--output <path>` still wins.
    let output_was_explicit = args.output.is_some();
    let output_dir: PathBuf = match args.output.clone() {
        Some(p) => {
            std::fs::create_dir_all(&p)
                .with_context(|| format!("create output dir {}", p.display()))?;
            p
        }
        None => {
            let p = default_output_for(&args.source, &args.to_provider);
            std::fs::create_dir_all(&p)
                .with_context(|| format!("create default output dir {}", p.display()))?;
            println!("📁 No --output specified; defaulting to {}", p.display());
            p
        }
    };
    let cwd_holder = TempDir::new().context("create temp cwd for backups")?;

    println!("📦 Emitting target HCL → {}", output_dir.display());

    // Schema-driven Generator (Article XIII rule 8 — no hardcoding).
    // Snapshot every loaded provider's latest schema and pass them in;
    // every resource type those schemas describe becomes emittable via
    // the generic flat-attribute fallback. Hand-curated templates still
    // take precedence for types needing custom HCL shaping.
    let mut schemas: Vec<std::sync::Arc<terrashift_knowledge::ProviderSchema>> = Vec::new();
    if let Ok(providers) = knowledge.schema_store.list_providers().await {
        for provider in providers {
            if let Ok(versions) = knowledge.schema_store.list_versions(&provider).await {
                if let Some(version) = versions.into_iter().next() {
                    if let Ok(schema) = knowledge
                        .schema_store
                        .fetch_provider_schema(&provider, &version)
                        .await
                    {
                        schemas.push(std::sync::Arc::new(schema));
                    }
                }
            }
        }
    }
    let generator = if schemas.is_empty() {
        println!(
            "⚠ No provider schemas cached — falling back to hand-curated template set only. \
             Run `terrashift schemas sync --provider azurerm --version 4.71.0` to expand coverage."
        );
        Generator::new()
    } else {
        Generator::with_schemas(schemas)
    };

    // Best-effort per-resource emission: skip resources with schema
    // gaps (missing required attrs / missing template) but keep going.
    // Stage 2 narrowing — the Recovery agent (S10) closes these gaps
    // with a follow-up LLM round-trip; for now we surface them
    // diagnostically and emit what we can.
    let mut emitted_files: Vec<PathBuf> = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();

    for r in &plan.resources {
        // Article III gate: if Validator flagged this resource, skip emission
        // and report the gap. Recovery agent (S10) is what closes these by
        // re-prompting the Mapper with the gap context; Stage 1 reports them.
        if let Some(errs) = validator_errors_by_addr.get(&r.target_addr) {
            // Surface the first error for the summary; chain is preserved
            // in the full ValidatorReport for future Recovery use.
            let first = errs
                .first()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Validator: unknown error".to_string());
            skipped.push((r.target_addr.clone(), format!("Validator — {first}")));
            continue;
        }

        let single_plan = terrashift_engine::mapper::MappingPlan {
            run_id: plan.run_id,
            source_provider: plan.source_provider.clone(),
            target_provider: plan.target_provider.clone(),
            resources: vec![r.clone()],
        };
        let single_out = TempDir::new().context("per-resource temp out")?;
        match generator.generate(cwd_holder.path(), single_out.path(), &single_plan) {
            Ok(art) => {
                for src in &art.files {
                    let name = src.file_name().unwrap_or_default();
                    let dst = output_dir.join(name);
                    if let Ok(content) = std::fs::read_to_string(src) {
                        let existing = std::fs::read_to_string(&dst).unwrap_or_default();
                        let combined = if existing.is_empty() {
                            content
                        } else {
                            format!("{existing}\n{content}")
                        };
                        let _ = std::fs::write(&dst, combined);
                        if !emitted_files.contains(&dst) {
                            emitted_files.push(dst);
                        }
                    }
                }
            }
            Err(err) => {
                skipped.push((r.target_addr.clone(), format!("Generator — {err}")));
            }
        }
    }

    // ─── 6. Summary ──────────────────────────────────────────────────
    println!("\n──────────── Migration summary ────────────");
    println!("Source resources : {}", total_resources);
    println!("Mapped to target : {}", plan.resources.len());
    println!("HCL files emitted: {}", emitted_files.len());
    println!("Skipped (gaps)   : {}", skipped.len());
    println!("Output directory : {}", output_dir.display());

    if !emitted_files.is_empty() {
        println!("\nEmitted files:");
        for path in &emitted_files {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            println!("  📄 {} ({} bytes)", name, bytes);
        }
    }

    if !skipped.is_empty() {
        println!("\nResources skipped (Recovery agent target — S10):");
        for (addr, why) in &skipped {
            println!("  ⚠ {addr}: {why}");
        }
    }

    // r07-mvp-closure: no more "files disappear on exit" footnote. The
    // default-output is now a real persistent path next to the source.
    let _ = output_was_explicit;

    Ok(())
}

/// Pull the resource address out of any `ValidationError` variant.
/// All current variants carry `addr: String` as the first field; this
/// helper centralises the projection so adding a new variant doesn't
/// silently break per-resource attribution.
fn addr_of_validation_error(err: &ValidationError) -> &str {
    match err {
        ValidationError::UnknownResourceType { addr, .. } => addr,
        ValidationError::UnknownAttribute { addr, .. } => addr,
        ValidationError::MissingRequiredAttribute { addr, .. } => addr,
    }
}

/// Compute the default output directory for a migration when the user
/// doesn't pass `--output`. Convention: `<source-stem>-<target-provider>/`
/// next to the source. Falls back to `./terrashift-output-<target>/` when
/// the source's file_name is missing or trivial (`.`, `..`).
///
/// r07-mvp-closure / FR-5. Test cases pinned in `mod tests` below.
fn default_output_for(source: &Path, target_provider: &str) -> PathBuf {
    let stem = source.file_name().and_then(|s| s.to_str());
    match stem {
        Some(s) if !s.is_empty() && s != "." && s != ".." => {
            let parent = source.parent().filter(|p| !p.as_os_str().is_empty());
            let new_name = format!("{s}-{target_provider}");
            match parent {
                Some(p) => p.join(new_name),
                None => PathBuf::from(format!("./{new_name}")),
            }
        }
        _ => PathBuf::from(format!("./terrashift-output-{target_provider}")),
    }
}

fn parse_tier(s: &str) -> Result<Tier> {
    match s.to_ascii_lowercase().as_str() {
        "eco" => Ok(Tier::Eco),
        "smart" => Ok(Tier::Smart),
        other => Err(anyhow::anyhow!(
            "invalid --tier '{other}': expected 'eco' or 'smart'"
        )),
    }
}

#[cfg(test)]
mod tests {
    //! r07-mvp-closure / FR-5: default-output convention pinned by tests.
    use super::*;

    #[test]
    fn default_output_for_normal_relative_path() {
        let p = default_output_for(Path::new("./aws-app"), "azurerm");
        assert_eq!(p, PathBuf::from("./aws-app-azurerm"));
    }

    #[test]
    fn default_output_for_relative_no_dot_prefix() {
        let p = default_output_for(Path::new("aws-app"), "azurerm");
        // No parent → ./aws-app-azurerm fallback.
        assert_eq!(p, PathBuf::from("./aws-app-azurerm"));
    }

    #[test]
    fn default_output_for_absolute_path() {
        let p = default_output_for(Path::new("/home/user/aws-app"), "azurerm");
        assert_eq!(p, PathBuf::from("/home/user/aws-app-azurerm"));
    }

    #[test]
    fn default_output_for_dot_uses_terrashift_output_fallback() {
        let p = default_output_for(Path::new("."), "azurerm");
        assert_eq!(p, PathBuf::from("./terrashift-output-azurerm"));
    }

    #[test]
    fn default_output_for_dotdot_uses_fallback() {
        let p = default_output_for(Path::new(".."), "azurerm");
        assert_eq!(p, PathBuf::from("./terrashift-output-azurerm"));
    }

    #[test]
    fn default_output_for_target_provider_appears_in_name() {
        // For each provider we support, the default-output name should
        // contain the provider key.
        for tp in ["azurerm", "aws", "google"] {
            let p = default_output_for(Path::new("./tf-tree"), tp);
            assert!(
                p.to_string_lossy().contains(tp),
                "default output for target '{tp}' should contain '{tp}', got {p:?}"
            );
        }
    }

    #[test]
    fn default_output_for_path_with_trailing_slash() {
        // PathBuf::file_name() drops the trailing slash naturally.
        let p = default_output_for(Path::new("./aws-app/"), "azurerm");
        assert_eq!(p, PathBuf::from("./aws-app-azurerm"));
    }
}
