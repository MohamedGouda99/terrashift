//! Directory-level golden comparison.
//!
//! Compares two directories file-by-file with byte-equality. Mismatches
//! produce a unified diff via the `similar` crate.
//!
//! TODO(S7): switch to `insta` once goldens grow past ~10 (per
//! specs/012-eval-framework/clarify.md Q2). `insta` provides reviewable
//! snapshots and `cargo insta review` workflow that scales better than
//! raw byte diffs in CI logs.
//!
//! Constitution: Article IV (loud failures — diffs name the file path),
//! Article VI (deterministic Generator output makes byte-equality safe).

use crate::errors::EvalError;
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Output of a directory-level comparison.
#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub passed: bool,
    /// `None` when `passed`. Otherwise a unified diff (or summary of which
    /// files were missing/extra) suitable for CI logs.
    pub diff: Option<String>,
}

/// Walk both directories one level deep, compare every file by name +
/// byte content. Returns:
/// - `passed: true` when both directory listings match AND every file is
///   byte-identical.
/// - `passed: false` with a diff naming missing/extra files and unified
///   diffs of mismatched contents.
pub fn compare_directories(
    actual_dir: &Path,
    expected_dir: &Path,
) -> Result<ComparisonResult, EvalError> {
    let actual_files = list_tf_files(actual_dir)?;
    let expected_files = list_tf_files(expected_dir)?;

    let actual_names: BTreeSet<String> = actual_files.iter().map(|p| filename_string(p)).collect();
    let expected_names: BTreeSet<String> =
        expected_files.iter().map(|p| filename_string(p)).collect();

    let mut diff_chunks: Vec<String> = Vec::new();

    for missing in expected_names.difference(&actual_names) {
        diff_chunks.push(format!("- missing from actual: {}", missing));
    }
    for extra in actual_names.difference(&expected_names) {
        diff_chunks.push(format!("+ unexpected in actual: {}", extra));
    }

    for filename in expected_names.intersection(&actual_names) {
        let actual_path = actual_dir.join(filename);
        let expected_path = expected_dir.join(filename);
        let actual_bytes = std::fs::read(&actual_path).map_err(|e| EvalError::Io {
            path: actual_path.clone(),
            source: e,
        })?;
        let expected_bytes = std::fs::read(&expected_path).map_err(|e| EvalError::Io {
            path: expected_path.clone(),
            source: e,
        })?;
        if actual_bytes != expected_bytes {
            let actual_text = String::from_utf8_lossy(&actual_bytes);
            let expected_text = String::from_utf8_lossy(&expected_bytes);
            diff_chunks.push(format!(
                "~ {}:\n{}",
                filename,
                unified_diff(&expected_text, &actual_text, filename)
            ));
        }
    }

    if diff_chunks.is_empty() {
        Ok(ComparisonResult {
            passed: true,
            diff: None,
        })
    } else {
        Ok(ComparisonResult {
            passed: false,
            diff: Some(diff_chunks.join("\n\n")),
        })
    }
}

/// Render a unified diff suitable for CI logs. Header lines name the file.
fn unified_diff(expected: &str, actual: &str, filename: &str) -> String {
    let diff = TextDiff::from_lines(expected, actual);
    let mut out = String::new();
    out.push_str(&format!("--- expected/{}\n", filename));
    out.push_str(&format!("+++ actual/{}\n", filename));
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        out.push_str(&format!("{}{}", sign, change));
        if !change.value().ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn list_tf_files(dir: &Path) -> Result<Vec<PathBuf>, EvalError> {
    if !dir.is_dir() {
        return Err(EvalError::Io {
            path: dir.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "directory not found"),
        });
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| EvalError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "tf")
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    files.sort();
    Ok(files)
}

fn filename_string(p: &Path) -> String {
    p.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("<no_name>")
        .to_string()
}
