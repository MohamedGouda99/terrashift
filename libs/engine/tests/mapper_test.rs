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
