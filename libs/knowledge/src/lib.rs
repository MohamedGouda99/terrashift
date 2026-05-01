//! Terraform Registry mirror — schema cache (Stage 1) + RAG (Stage 3+).
//!
//! Pattern: stakpak_arch.md section 9 (SessionStorage trait shape adapted
//! for schema cache).
//!
//! Constitution: Article VI (knowledge layer integrity — version pinning;
//! same migration today produces same output six months from now), Article X.
//!
//! Stage 1 scope:
//! - Schema cache only (SchemaStore trait + LocalSchemaStore SQLite impl)
//! - `search_mappings()` is a stub returning `Vec::new()` so Mapper code
//!   compiles against the final API; Stage 3 swap is a single function rewrite.
//!
//! Modules to be filled in by P-NN prompts:
//! - `schema_cache.rs` — SchemaStore trait + LocalSchemaStore (P-07)
//! - `registry_client.rs` — registry.terraform.io API client (P-07)
//! - `mappings.rs` — cross-cloud mapping corpus (Stage 3)
//! - `vector_store.rs` — LanceDB integration (Stage 3)
//! - `retrieval.rs` — exact → vector → LLM fallback (per LLD-3)

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
