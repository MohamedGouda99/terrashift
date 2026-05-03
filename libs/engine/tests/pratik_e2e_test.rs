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

use terrashift_ai::{Profile, RealClient, Resolver, Tier};
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

    // ─── 3. Build KnowledgeService (stubs — RAG retrieval returns 0 hits;
    //         tests Mapper without leaning on schema cache) ───────────
    let knowledge = build_stub_knowledge().await;

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

    // ─── 7. Soft assertions ──────────────────────────────────────────
    // These are deliberately lenient — we want the test to print rich
    // diagnostics without flaking on individual LLM-output variations.
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
    println!(
        "\n✅ Test passed (≥1 azurerm_* resource emitted; {} of {} mapped resources are azurerm_*)\n",
        azurerm_count,
        plan.resources.len()
    );
}

async fn build_stub_knowledge() -> KnowledgeService {
    let store = Arc::new(
        LocalSchemaStore::in_memory()
            .await
            .expect("in-memory schema store"),
    );
    let vector = Arc::new(InMemoryVectorStore::new(VECTOR_DIM));
    let embedder = Arc::new(StubEmbeddingService::with_dimension(VECTOR_DIM));
    let fetcher = Arc::new(StubSchemaFetcher::new());
    KnowledgeService::new(store, vector, embedder, fetcher)
}
