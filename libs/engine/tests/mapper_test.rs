// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Mapper integration tests — single-LLM-call structured output.
//!
//! All tests offline (use `StubClient` from `terrashift-ai`). The real
//! Groq round-trip is the integration test in `libs/ai/tests/llm_client_test.rs`
//! (P-03), `#[ignore]`d until `GROQ_API_KEY` is set.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use terrashift_ai::{AiError, CompletionMetadata, LlmClient, StubClient, Tier};
use terrashift_engine::mapper::{
    ContextReducer, Mapper, MapperCache, MapperError, Message, PassthroughContextReducer,
};
use terrashift_engine::scanner::{EstateInventory, Resource, ScannedFile, SourceSpan};
use terrashift_knowledge::embedding::StubEmbeddingService;
use terrashift_knowledge::local_schema_store::LocalSchemaStore;
use terrashift_knowledge::registry_client::StubSchemaFetcher;
use terrashift_knowledge::vector_store::InMemoryVectorStore;
use terrashift_knowledge::KnowledgeService;

// ─────────────────────────────────────────────────────────────────────
// Test fixtures
// ─────────────────────────────────────────────────────────────────────

fn make_test_inventory(resources: Vec<(&str, &str)>) -> EstateInventory {
    let scanned_resources: Vec<Resource> = resources
        .into_iter()
        .map(|(rtype, name)| Resource {
            resource_type: rtype.to_string(),
            name: name.to_string(),
            attributes: BTreeMap::new(),
            source_span: SourceSpan::default(),
        })
        .collect();
    let mut file = ScannedFile::new(std::path::PathBuf::from("test.tf"));
    file.resources = scanned_resources;
    EstateInventory {
        root_dir: std::path::PathBuf::from("/tmp/test"),
        files: vec![file],
    }
}

const CANNED_PLAN_JSON: &str = r#"{
  "run_id": "00000000-0000-0000-0000-000000000001",
  "source_provider": "google",
  "target_provider": "aws",
  "resources": [
    {
      "source_addr": "google_compute_network.main",
      "target_addr": "aws_vpc.main",
      "target_type": "aws_vpc",
      "target_name": "main",
      "attributes": { "cidr_block": { "string": "10.0.0.0/16" } },
      "dependencies": []
    }
  ]
}"#;

async fn make_test_knowledge() -> KnowledgeService {
    let store = LocalSchemaStore::in_memory().await.unwrap();
    KnowledgeService::new(
        Arc::new(store),
        Arc::new(InMemoryVectorStore::new(1024)),
        Arc::new(StubEmbeddingService::new()),
        Arc::new(StubSchemaFetcher::new()),
    )
}

// ─────────────────────────────────────────────────────────────────────
// US1 — Mapper happy path
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn happy_path_with_stub_client() {
    let estate = make_test_inventory(vec![("google_compute_network", "main")]);
    let knowledge = make_test_knowledge().await;
    let llm = StubClient::new().with_default(CANNED_PLAN_JSON);
    let reducer = PassthroughContextReducer;
    let mut cache = MapperCache::new();

    let mapper = Mapper::new("google", "aws");
    let plan = mapper
        .map(&estate, &knowledge, &llm, &reducer, &mut cache)
        .await
        .unwrap();

    assert_eq!(plan.source_provider, "google");
    assert_eq!(plan.target_provider, "aws");
    assert_eq!(plan.resources.len(), 1);
    assert_eq!(plan.resources[0].target_addr, "aws_vpc.main");
}

#[tokio::test]
async fn empty_inventory_skips_llm_call() {
    let estate = EstateInventory {
        root_dir: std::path::PathBuf::from("/tmp/empty"),
        files: vec![],
    };
    let knowledge = make_test_knowledge().await;
    let llm = CountingLlmClient::new("UNUSED");
    let reducer = PassthroughContextReducer;
    let mut cache = MapperCache::new();

    let mapper = Mapper::new("google", "aws");
    let plan = mapper
        .map(&estate, &knowledge, &llm, &reducer, &mut cache)
        .await
        .unwrap();

    assert!(plan.resources.is_empty(), "empty inventory → empty plan");
    assert_eq!(
        llm.count(),
        0,
        "Article XII rule 1: no LLM call for empty inventory"
    );
}

#[tokio::test]
async fn malformed_json_is_loud_error() {
    let estate = make_test_inventory(vec![("google_compute_network", "main")]);
    let knowledge = make_test_knowledge().await;
    let llm = StubClient::new().with_default("not valid JSON {");
    let reducer = PassthroughContextReducer;
    let mut cache = MapperCache::new();

    let mapper = Mapper::new("google", "aws");
    let result = mapper
        .map(&estate, &knowledge, &llm, &reducer, &mut cache)
        .await;
    match result {
        Err(MapperError::MalformedResponse { reason, sample }) => {
            assert!(!reason.is_empty(), "reason is non-empty");
            assert!(
                sample.contains("not valid JSON"),
                "sample includes raw response: {sample}"
            );
        }
        other => panic!("expected MalformedResponse, got {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────
// US2 — Article XIII rule 1 enforcement
// ─────────────────────────────────────────────────────────────────────

struct CountingContextReducer {
    count: AtomicUsize,
}

impl CountingContextReducer {
    fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
        }
    }
    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl ContextReducer for CountingContextReducer {
    fn reduce(&self, messages: Vec<Message>) -> Vec<Message> {
        self.count.fetch_add(1, Ordering::SeqCst);
        messages
    }
}

#[tokio::test]
async fn reducer_is_on_path() {
    let estate = make_test_inventory(vec![("google_compute_network", "main")]);
    let knowledge = make_test_knowledge().await;
    let llm = StubClient::new().with_default(CANNED_PLAN_JSON);
    let reducer = CountingContextReducer::new();
    let mut cache = MapperCache::new();

    let mapper = Mapper::new("google", "aws");
    mapper
        .map(&estate, &knowledge, &llm, &reducer, &mut cache)
        .await
        .unwrap();

    assert_eq!(
        reducer.count(),
        1,
        "Article XIII rule 1: reducer.reduce is on the path exactly once per non-cached call"
    );
}

struct CountingLlmClient {
    count: AtomicUsize,
    response: String,
}

impl CountingLlmClient {
    fn new(response: &str) -> Self {
        Self {
            count: AtomicUsize::new(0),
            response: response.to_string(),
        }
    }
    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl LlmClient for CountingLlmClient {
    async fn complete(
        &self,
        tier: Tier,
        _prompt: &str,
    ) -> Result<(String, CompletionMetadata), AiError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        let meta = CompletionMetadata::stub("counting", "stub-v1", "stub://counter", tier);
        Ok((self.response.clone(), meta))
    }
}

#[tokio::test]
async fn cache_hit_avoids_llm_and_reducer() {
    let estate = make_test_inventory(vec![("google_compute_network", "main")]);
    let knowledge = make_test_knowledge().await;
    let llm = CountingLlmClient::new(CANNED_PLAN_JSON);
    let reducer = CountingContextReducer::new();
    let mut cache = MapperCache::new();

    let mapper = Mapper::new("google", "aws");
    // First call: hits LLM + reducer.
    mapper
        .map(&estate, &knowledge, &llm, &reducer, &mut cache)
        .await
        .unwrap();
    assert_eq!(llm.count(), 1, "first call hits LLM");
    assert_eq!(reducer.count(), 1, "first call hits reducer");

    // Second call with same inventory: cache hit, NO additional LLM or reducer call.
    let plan2 = mapper
        .map(&estate, &knowledge, &llm, &reducer, &mut cache)
        .await
        .unwrap();
    assert_eq!(llm.count(), 1, "Article XII rule 2: cache hit avoids LLM");
    assert_eq!(
        reducer.count(),
        1,
        "cache hit short-circuits before reducer"
    );
    assert_eq!(plan2.resources[0].target_addr, "aws_vpc.main");
}

// ─────────────────────────────────────────────────────────────────────
// US3 — Knowledge hits inform the prompt
// ─────────────────────────────────────────────────────────────────────

struct RecordingLlmClient {
    captured_prompt: Mutex<String>,
    response: String,
}

impl RecordingLlmClient {
    fn new(response: &str) -> Self {
        Self {
            captured_prompt: Mutex::new(String::new()),
            response: response.to_string(),
        }
    }
    fn captured_prompt(&self) -> String {
        self.captured_prompt.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmClient for RecordingLlmClient {
    async fn complete(
        &self,
        tier: Tier,
        prompt: &str,
    ) -> Result<(String, CompletionMetadata), AiError> {
        *self.captured_prompt.lock().unwrap() = prompt.to_string();
        let meta = CompletionMetadata::stub("recording", "stub-v1", "stub://recorder", tier);
        Ok((self.response.clone(), meta))
    }
}

#[tokio::test]
async fn prompt_contains_inventory_resources() {
    let estate = make_test_inventory(vec![
        ("google_compute_network", "main"),
        ("google_storage_bucket", "assets"),
    ]);
    let knowledge = make_test_knowledge().await;
    let llm = RecordingLlmClient::new(CANNED_PLAN_JSON);
    let reducer = PassthroughContextReducer;
    let mut cache = MapperCache::new();

    let mapper = Mapper::new("google", "aws");
    mapper
        .map(&estate, &knowledge, &llm, &reducer, &mut cache)
        .await
        .unwrap();

    let prompt = llm.captured_prompt();
    assert!(
        prompt.contains("google_compute_network.main"),
        "prompt mentions first source resource: {prompt}"
    );
    assert!(
        prompt.contains("google_storage_bucket.assets"),
        "prompt mentions second source resource: {prompt}"
    );
    assert!(prompt.contains("[system]"), "prompt has system role tag");
    assert!(prompt.contains("[user]"), "prompt has user role tag");
    assert!(
        prompt.contains("MappingPlan"),
        "system prompt names the schema"
    );
}

// ─────────────────────────────────────────────────────────────────────
// SC-006 — estate_cache_key is byte-stable
// ─────────────────────────────────────────────────────────────────────

#[test]
fn estate_cache_key_is_stable_across_runs() {
    let estate = make_test_inventory(vec![("google_compute_network", "main")]);
    let key1 = terrashift_engine::mapper::estate_cache_key(&estate).unwrap();
    let key2 = terrashift_engine::mapper::estate_cache_key(&estate).unwrap();
    assert_eq!(key1, key2, "Article VI: cache key byte-stable across runs");
    assert_eq!(key1.len(), 64, "sha256 hex digest is 64 hex chars");
}

// ─────────────────────────────────────────────────────────────────────
// R5 — Mapper-level RealClient integration test (S4-close).
//
// Wires the actual `RealClient` (P-03) instead of `StubClient` and
// runs Mapper.map() against the real Groq endpoint. `#[ignore]`'d AND
// env-var-gated so default `cargo test` stays offline-safe; activates
// with `cargo test -- --ignored` once GROQ_API_KEY is set.
//
// This test is the swap-pattern proof: in production code the only
// difference between Stage 1 (StubClient) and S4-close (RealClient)
// is *which type implements LlmClient* — `Mapper::map()` does not
// know or care. When this test passes:
// - Stage 1 P-16 criterion #3 (re-runs identical output) graduates
//   PARTIAL → PASS for the LLM-side determinism
//   (cache hit on second call avoids the LLM, verified by counter
//   inside CountingLlmClient — but here we use RealClient directly,
//   so the assertion is "first call returns parseable JSON; second
//   call hits the cache without network")
// - Criterion #4 (token cost) gets real data into the eval
//   baseline.json refresh
//
// Per the user's R5 ticket, this `#[ignore]`'d test is the canonical
// place where S4-close validation fires.
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "real Groq round-trip via Mapper; requires GROQ_API_KEY. Activate with: cargo test -p terrashift-engine real_mapper_round_trip_via_groq -- --ignored"]
async fn real_mapper_round_trip_via_groq() {
    // Belt-and-braces gate: skip silently if the env var isn't set even
    // when --ignored is passed. Avoids surprising failures when the
    // operator runs `cargo test -- --ignored` against an unrelated
    // crate's ignored tests.
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!(
            "[skip] real_mapper_round_trip_via_groq: GROQ_API_KEY not set; \
             this test is intentionally a no-op without the key."
        );
        return;
    }

    use terrashift_ai::{Profile, RealClient, Resolver};

    // Same Stage-1 BYOK profile as P-03's integration test in
    // libs/ai/tests/llm_client_test.rs::real_groq_completes.
    let profile_toml = r#"
[profiles.default]
model = "groq/llama-3.3-70b-versatile"

  [profiles.default.tiers]
  eco   = "groq/llama-3.3-70b-versatile"
  smart = "groq/llama-3.3-70b-versatile"

  [profiles.default.providers.groq]
  type = "openai-compatible"
  api_endpoint = "https://api.groq.com/openai/v1"
  api_key_env = "GROQ_API_KEY"
"#;

    let profile = Profile::from_toml(profile_toml).unwrap();
    let resolved = Resolver::resolve_for_tier(&profile, None, None, Tier::Eco).unwrap();
    let llm = RealClient::new(resolved).expect("RealClient construction (openai-compatible)");

    // Tiny inventory — single GCP network — keeps token cost minimal
    // for repeated CI runs once GROQ_API_KEY lands.
    let estate = make_test_inventory(vec![("google_compute_network", "main")]);
    let knowledge = make_test_knowledge().await;
    let reducer = PassthroughContextReducer;
    let mut cache = MapperCache::new();

    let mapper = Mapper::new("google", "aws");
    let plan = mapper
        .map(&estate, &knowledge, &llm, &reducer, &mut cache)
        .await
        .expect("Mapper round-trip via Groq returns a parseable MappingPlan");

    // The LLM may pick any reasonable mapping; we only assert the
    // structural contract (Validator P-06 enforces deeper checks).
    assert_eq!(plan.source_provider, "google");
    assert_eq!(plan.target_provider, "aws");
    assert!(
        !plan.resources.is_empty(),
        "real Mapper produced at least one mapped resource"
    );

    // Cache-hit invariant (Article XII rule 2): second call must hit
    // the cache without re-invoking the LLM. We can't directly count
    // RealClient's network calls, but we can verify the *output* is
    // byte-identical (which proves no new LLM call happened — the
    // cache returned the same plan).
    let plan2 = mapper
        .map(&estate, &knowledge, &llm, &reducer, &mut cache)
        .await
        .expect("second map call (cache hit)");

    // The cache returns a clone of the same plan; the run_id will
    // match because we cached the entire MappingPlan including run_id.
    assert_eq!(
        serde_json::to_string(&plan).unwrap(),
        serde_json::to_string(&plan2).unwrap(),
        "cache hit returns byte-identical plan (Article XII rule 2)"
    );
}
