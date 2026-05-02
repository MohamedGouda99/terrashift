//! Recursive `.tf` file discovery.
//!
//! Pattern: terrashift_plan.md §5 (Scanner discovery phase).
//! Constitution: Article IV (loud io errors), XIII rule 3 (no unwrap).

use crate::scanner::errors::ScannerError;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Walk `root` recursively and return every `.tf` file path. Skips `.git/`,
/// `.terraform/`, `target/`, and any directory whose name starts with `.`.
///
/// Order is filesystem-dependent (WalkDir's default). Caller should sort if
/// stable order matters (tests do).
pub fn discover_tf_files(root: &Path) -> Result<Vec<PathBuf>, ScannerError> {
    if !root.exists() {
        return Err(ScannerError::Io {
            path: root.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("scan root does not exist: {}", root.display()),
            ),
        });
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_hidden_or_skipped(e))
    {
        let entry = entry.map_err(|e| ScannerError::Io {
            path: e
                .path()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.to_path_buf()),
            source: e.into_io_error().unwrap_or_else(|| {
                std::io::Error::other("walkdir failed without io error context")
            }),
        })?;

        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "tf") {
            files.push(path.to_path_buf());
        }
    }
    Ok(files)
}

fn is_hidden_or_skipped(entry: &walkdir::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();

    // Always allow the root entry (depth 0)
    if entry.depth() == 0 {
        return false;
    }

    // Skip hidden dirs/files
    if name.starts_with('.') {
        return true;
    }

    // Skip generated directories
    matches!(name.as_ref(), ".terraform" | "target" | "node_modules")
}
