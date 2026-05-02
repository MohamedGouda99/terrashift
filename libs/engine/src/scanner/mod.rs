//! Scanner — deterministic HCL parser. First component of the migration
//! pipeline (HLD-2 box 1).
//!
//! Pattern: terrashift_plan.md §5, §11.
//! Constitution: Article I (deterministic, not an agent), IV (failures loud).
//!
//! ## Public API
//!
//! ```no_run
//! use terrashift_engine::scanner::Scanner;
//! use std::path::Path;
//!
//! let inventory = Scanner::scan(Path::new("./infra"))?;
//! println!("found {} resources", inventory.resource_count());
//! # Ok::<_, terrashift_engine::scanner::ScannerError>(())
//! ```

pub mod errors;
pub mod inventory;
pub mod parser;
pub mod walker;

pub use errors::ScannerError;
pub use inventory::{
    DataSource, EstateInventory, Module, Output, Provider, Resource, ScannedFile, SourceSpan,
    Variable,
};

use std::path::Path;

/// Scanner facade — entry point for `Scanner::scan(root)`.
///
/// Stateless; no fields. The struct gives a stable public surface (callers
/// don't need to import a free function).
pub struct Scanner;

impl Scanner {
    /// Walk `root` recursively, parse every `.tf` file, return the inventory.
    /// First hard-fail (e.g., `dynamic` block) returns Err.
    pub fn scan(root: &Path) -> Result<EstateInventory, ScannerError> {
        let files = walker::discover_tf_files(root)?;
        let mut inventory = EstateInventory {
            root_dir: root.to_path_buf(),
            files: Vec::with_capacity(files.len()),
        };

        for path in files {
            let content = std::fs::read_to_string(&path).map_err(|e| ScannerError::Io {
                path: path.clone(),
                source: e,
            })?;
            let scanned = parser::parse_file(&path, &content)?;
            inventory.files.push(scanned);
        }

        Ok(inventory)
    }

    /// Parse a single file from in-memory content. Useful for tests + the
    /// future REPL/diagnostic surface.
    pub fn parse_file(path: &Path, content: &str) -> Result<ScannedFile, ScannerError> {
        parser::parse_file(path, content)
    }
}
