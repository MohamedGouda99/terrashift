//! `terrashift migrate` — run the Scanner → Mapper → Generator pipeline.
//!
//! Stage 2 narrowing: skips Validator + Recovery + Executor for now.
//! Validator integration lands when KnowledgeService schema versions
//! are pinned to real Terraform-registry pulls (run `terrashift
//! schemas sync` first to populate). Executor integration lands at
//! S5-close once Docker + cloud creds are wired.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tempfile::TempDir;
use terrashift_ai::Tier;
use terrashift_engine::generator::Generator;
use terrashift_engine::mapper::{Mapper, MapperCache, PassthroughContextReducer};
use terrashift_engine::scanner::Scanner;

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
    let knowledge = build_knowledge_service().await?;

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
    let plan = mapper
        .map(&inv, &knowledge, &llm, &reducer, &mut cache)
        .await
        .context("Mapper LLM round-trip")?;
    let elapsed = started.elapsed();

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
    let output_was_explicit = args.output.is_some();
    let (output_dir, _temp_holder) = match args.output.clone() {
        Some(p) => {
            std::fs::create_dir_all(&p)
                .with_context(|| format!("create output dir {}", p.display()))?;
            (p, None::<TempDir>)
        }
        None => {
            let tmp = TempDir::new().context("create temp output dir")?;
            (tmp.path().to_path_buf(), Some(tmp))
        }
    };
    let cwd_holder = TempDir::new().context("create temp cwd for backups")?;

    println!("📦 Emitting target HCL → {}", output_dir.display());
    let generator = Generator::new();

    // Best-effort per-resource emission: skip resources with schema
    // gaps (missing required attrs / missing template) but keep going.
    // Stage 2 narrowing — the Recovery agent (S10) closes these gaps
    // with a follow-up LLM round-trip; for now we surface them
    // diagnostically and emit what we can.
    let mut emitted_files: Vec<PathBuf> = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();

    for r in &plan.resources {
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
                skipped.push((r.target_addr.clone(), err.to_string()));
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

    if !output_was_explicit {
        println!(
            "\n💡 No --output specified; files were written to a temp dir that\n   \
             will be deleted on exit. Re-run with --output <path> to keep them."
        );
        // Hold onto _temp_holder so files survive at least until the
        // user reads the output. Drop happens at end of fn.
    }

    Ok(())
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
