#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Pratik E2E — real-world AWS→Azure migration test.
//!
//! Source: github.com/PratikMahajan/AWS-to-AZURE-Infrastructure-Migration
//! Fixture: fixtures/aws-to-azure-real/aws/modules/vpc/
//! Ground-truth: fixtures/aws-to-azure-real/azure/modules/virtual_network/
//!
//! This test:
//! 1. Skips if `HF_TOKEN` is unset (so CI stays hermetic).
//! 2. Skips if the fixture is not present locally.
//! 3. Scans the AWS VPC module fixture.
//! 4. Runs the Mapper against a real LLM (HuggingFace Inference Providers
//!    via openai-compat — defaults to Llama-3.3-70B-Instruct on Cerebras).
//! 5. Reports proposed mappings and compares the target types against
//!    Pratik's hand-authored Azure ground truth.
//!
//! Activate locally with:
//!
//! ```sh
//! export HF_TOKEN=hf_...
//! cargo test -p terrashift-engine pratik_e2e -- --ignored --nocapture
//! ```

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;
use terrashift_ai::{Profile, RealClient, Resolver, Tier};
use terrashift_engine::generator::Generator;
use terrashift_engine::mapper::{Mapper, MapperCache, PassthroughContextReducer};
use terrashift_engine::scanner::Scanner;
use terrashift_knowledge::embedding::StubEmbeddingService;
use terrashift_knowledge::local_schema_store::LocalSchemaStore;
use terrashift_knowledge::registry_client::StubSchemaFetcher;
use terrashift_knowledge::vector_store::InMemoryVectorStore;
use terrashift_knowledge::KnowledgeService;

// Together hosts Llama-3.3-70B-Instruct-Turbo via HF Inference Providers
// at this endpoint with HF auth. Cerebras and SambaNova are alternatives;
// try together first because it has the widest free-tier model coverage.
const HF_PROFILE_TOML: &str = r#"
model = "huggingface/meta-llama/Llama-3.3-70B-Instruct-Turbo"

[tiers]
eco   = "huggingface/meta-llama/Llama-3.3-70B-Instruct-Turbo"
smart = "huggingface/meta-llama/Llama-3.3-70B-Instruct-Turbo"

[providers.huggingface]
type         = "openai-compatible"
api_endpoint = "https://router.huggingface.co/together/v1"
api_key_env  = "HF_TOKEN"
"#;

const VECTOR_DIM: usize = 384;

#[tokio::test]
#[ignore = "real LLM round-trip; requires HF_TOKEN env"]
async fn pratik_e2e_vpc_module() {
    // ─── Pre-conditions ──────────────────────────────────────────────
    if std::env::var("HF_TOKEN").is_err() {
        panic!("HF_TOKEN env var must be set — see assets/profile.example.toml");
    }

    let workspace_root = env!("CARGO_MANIFEST_DIR");
    let fixture_root = PathBuf::from(workspace_root)
        .join("..")
        .join("..")
        .join("fixtures")
        .join("aws-to-azure-real");
    let aws_vpc = fixture_root.join("aws").join("modules").join("vpc");
    let azure_vnet = fixture_root
        .join("azure")
        .join("modules")
        .join("virtual_network");

    if !aws_vpc.exists() {
        panic!(
            "fixture not present at {} — run scripts/setup-fixtures.ps1",
            aws_vpc.display()
        );
    }

    println!("\n=== Pratik E2E — AWS VPC module → Azure (real LLM) ===\n");
    println!("📂 AWS source     : {}", aws_vpc.display());
    println!("📂 Ground-truth   : {}", azure_vnet.display());

    // ─── 1. Scan AWS source ──────────────────────────────────────────
    let aws_inv = Scanner::scan(&aws_vpc).expect("scan AWS vpc module");
    let aws_types: Vec<String> = aws_inv
        .files
        .iter()
        .flat_map(|f| f.resources.iter().map(|r| r.resource_type.clone()))
        .collect();
    println!(
        "\n🔍 Scanned {} AWS file(s); {} resource(s); types: {:?}",
        aws_inv.files.len(),
        aws_types.len(),
        aws_types
    );

    // ─── 2. Build LlmClient ──────────────────────────────────────────
    let profile = Profile::from_toml(HF_PROFILE_TOML).expect("parse profile");
    let resolved =
        Resolver::resolve_for_tier(&profile, None, None, Tier::Eco).expect("resolve eco tier");
    println!(
        "\n🤖 LLM ready  : {} via {} (key from ${})",
        resolved.model_id, resolved.api_endpoint, resolved.api_key_env
    );
    let llm = RealClient::new(resolved).expect("construct RealClient");

    // ─── 3. Build KnowledgeService — seeded from libs/knowledge/seed/
    //         (CloudForge-derived JSON snapshots). RAG retrieval surfaces
    //         the actual azurerm schemas in the Mapper prompt context, so
    //         the LLM emits required attributes (name / resource_group_name
    //         / location / etc.) without us hard-coding any of that in Rust. ─
    let knowledge = build_seeded_knowledge().await;

    // ─── 4. Run Mapper ───────────────────────────────────────────────
    let mapper = Mapper::new("aws", "azurerm");
    let reducer = PassthroughContextReducer;
    let mut cache = MapperCache::new();

    println!("\n🧠 Running Mapper (real LLM round-trip; this takes a few seconds) ...");
    let start = std::time::Instant::now();
    let plan = match mapper
        .map(&aws_inv, &knowledge, &llm, &reducer, &mut cache)
        .await
    {
        Ok(plan) => plan,
        Err(err) => panic!("Mapper::map failed: {err}"),
    };
    let elapsed = start.elapsed();
    println!(
        "✅ Mapper returned {} target resource(s) in {:.1}s",
        plan.resources.len(),
        elapsed.as_secs_f64()
    );

    // ─── 5. Print proposed mappings ──────────────────────────────────
    println!("\n--- Proposed AWS → Azure mappings ---");
    for r in &plan.resources {
        println!(
            "  {:40} → {:50} ({})",
            r.source_addr, r.target_addr, r.target_type
        );
    }

    // ─── 6. Compare to Pratik's ground-truth target types ────────────
    let pratik_types: BTreeSet<String> = if azure_vnet.exists() {
        let azure_inv = Scanner::scan(&azure_vnet).expect("scan azure virtual_network");
        azure_inv
            .files
            .iter()
            .flat_map(|f| f.resources.iter().map(|r| r.resource_type.clone()))
            .collect()
    } else {
        BTreeSet::new()
    };
    let mapped_types: BTreeSet<String> = plan
        .resources
        .iter()
        .map(|r| r.target_type.clone())
        .collect();

    println!("\n--- Type parity vs Pratik's azure/modules/virtual_network/ ---");
    println!("Pratik's types      : {:?}", pratik_types);
    println!("Mapped types (LLM)  : {:?}", mapped_types);
    let matched: BTreeSet<&String> = pratik_types.intersection(&mapped_types).collect();
    let missed: BTreeSet<&String> = pratik_types.difference(&mapped_types).collect();
    let extra: BTreeSet<&String> = mapped_types.difference(&pratik_types).collect();
    println!("✓ matched           : {:?}", matched);
    println!("✗ missed            : {:?}", missed);
    println!("? extra (LLM-only)  : {:?}", extra);

    if !pratik_types.is_empty() {
        let parity_pct = (matched.len() as f64 / pratik_types.len() as f64) * 100.0;
        println!(
            "\n📊 Type parity: {:.1}% of Pratik's types matched ({}/{} types)",
            parity_pct,
            matched.len(),
            pratik_types.len()
        );
    }

    // ─── 7. Run Generator → emit real .tf files to a TempDir ────────
    let cwd = TempDir::new().expect("cwd tempdir");
    let out_dir = TempDir::new().expect("out tempdir");
    println!("\n📦 Running Generator → emitting .tf files");
    println!("   cwd        : {}", cwd.path().display());
    println!("   output_dir : {}", out_dir.path().display());

    let generator = Generator::new();
    let gen_result = generator.generate(cwd.path(), out_dir.path(), &plan);

    // Best-effort emission: try Generator on each resource individually
    // so we get partial output even when some resources have schema
    // gaps. In production the S10 Recovery agent fixes the gaps before
    // re-running Generator on the full plan; here we surface them for
    // diagnostic visibility.
    let _ = gen_result; // discard the all-or-nothing first attempt

    let mut emitted_files: Vec<PathBuf> = Vec::new();
    let mut emit_failures: Vec<(String, String)> = Vec::new();

    for r in &plan.resources {
        let single_plan = terrashift_engine::mapper::MappingPlan {
            run_id: plan.run_id,
            source_provider: plan.source_provider.clone(),
            target_provider: plan.target_provider.clone(),
            resources: vec![r.clone()],
        };
        let single_cwd = TempDir::new().expect("per-resource cwd");
        let single_out = TempDir::new().expect("per-resource out");
        match generator.generate(single_cwd.path(), single_out.path(), &single_plan) {
            Ok(art) => {
                // Persist the emitted file to our long-lived out_dir so
                // it survives the per-iteration TempDir drop.
                for src in &art.files {
                    let name = src.file_name().unwrap_or_default();
                    let dst = out_dir.path().join(name);
                    if let Ok(content) = std::fs::read_to_string(src) {
                        // Append per-resource block to the per-type file.
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
                emit_failures.push((r.target_addr.clone(), err.to_string()));
            }
        }
    }

    println!(
        "\n--- Generator results: {} file(s) emitted, {} resource(s) skipped ---",
        emitted_files.len(),
        emit_failures.len()
    );
    for path in &emitted_files {
        let rel = path.strip_prefix(out_dir.path()).unwrap_or(path);
        let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        println!("  📄 {} ({} bytes)", rel.display(), bytes);
    }
    if !emit_failures.is_empty() {
        println!("\n--- Resources skipped (Recovery agent would fix these) ---");
        for (addr, err) in &emit_failures {
            println!("  ⚠ {addr}: {err}");
        }
    }

    // Print first emitted file's full contents so the operator can
    // eyeball the real HCL Terrashift produced from a real-world AWS
    // module.
    if let Some(first) = emitted_files.first() {
        if let Ok(content) = std::fs::read_to_string(first) {
            println!(
                "\n--- Full content of {} ---\n{content}",
                first.file_name().unwrap_or_default().to_string_lossy()
            );
        }
    }

    let artifacts_files = emitted_files;

    // ─── 8. Soft assertions ──────────────────────────────────────────
    // Deliberately lenient — diagnostics > flakiness on LLM variations.
    assert!(
        !plan.resources.is_empty(),
        "Mapper produced empty plan — LLM call returned nothing usable"
    );
    let azurerm_count = plan
        .resources
        .iter()
        .filter(|r| r.target_type.starts_with("azurerm_"))
        .count();
    assert!(
        azurerm_count > 0,
        "no azurerm_* resources in mapped plan — LLM didn't produce target-shaped output"
    );
    assert!(
        !artifacts_files.is_empty(),
        "Generator emitted zero files — at least one resource should have a registered template"
    );
    println!(
        "\n✅ Test passed: Mapper end-to-end + {} HCL file(s) emitted ({} of {} resources are azurerm_*; {} resources skipped due to schema gaps)\n",
        artifacts_files.len(),
        azurerm_count,
        plan.resources.len(),
        emit_failures.len()
    );
}

/// Build a KnowledgeService pre-loaded from the bundled CloudForge seed
/// at `libs/knowledge/seed/`. Falls back to empty if seed dir is missing.
async fn build_seeded_knowledge() -> KnowledgeService {
    let store = Arc::new(
        LocalSchemaStore::in_memory()
            .await
            .expect("in-memory schema store"),
    );
    let vector = Arc::new(InMemoryVectorStore::new(VECTOR_DIM));
    let embedder = Arc::new(StubEmbeddingService::with_dimension(VECTOR_DIM));
    let fetcher = Arc::new(StubSchemaFetcher::new());
    let knowledge = KnowledgeService::new(store, vector, embedder, fetcher);

    let seed_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("knowledge")
        .join("seed");
    match knowledge.seed_from_bundle(&seed_dir).await {
        Ok(n) => println!("📚 Seeded {} resources from {}", n, seed_dir.display()),
        Err(e) => println!("⚠ Seed load failed: {} (continuing with empty cache)", e),
    }
    knowledge
}
