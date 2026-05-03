//! Reversible file operations — move-to-backup with EXDEV fallback.
//!
//! Pattern: the architecture reference §28 (reversible file operations).
//! Source: the reference codebase (see ATTRIBUTIONS.md)
//!         (verbatim shape; Terrashift adds an EXDEV copy+remove fallback
//!         that the reference lacks per §28's caveat list).
//! Constitution: Article IV (loud failures), Article V (reversible),
//!               Article IX (backups archival, never auto-deleted),
//!               Article XIII rule 3 (no unwrap/expect/string-slice).

use crate::generator::errors::BackupError;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Where backups for a given run live. the reference's path is
/// `.the reference/session/backups/{uuid}/` (per `file_backup_manager.rs:21-23`).
/// Terrashift adds the `run_id` layer so audit replay can scope by run —
/// orphan backups without a run scope would be a bug (Article IV).
pub fn backup_root(cwd: &Path, run_id: Uuid) -> PathBuf {
    cwd.join(".terrashift")
        .join("runs")
        .join(run_id.to_string())
        .join("backups")
}

/// Move `path` to `<backup_root>/<op_uuid>/<filename>`. Returns the new
/// backup path.
///
/// Mirrors `FileBackupManager::move_local_path_to_backup` at
/// `the reference codebase (see ATTRIBUTIONS.md)`. The
/// per-call `op_uuid` is generated fresh (line 20 in the the reference source).
///
/// **Deviation from the reference (deliberate):** when `std::fs::rename` fails
/// because the source and destination are on different mounts (EXDEV on
/// Linux/macOS, ERROR_NOT_SAME_DEVICE on Windows), we fall back to
/// `copy + remove_file` instead of bubbling the OS error up. the reference
/// (`file_backup_manager.rs:35-41`) bails on EXDEV; that turns a backup
/// into a hard error in cases where the user's `output_dir` happens to
/// be on a different mount than `cwd` — common on Windows with separate
/// drives, on Linux with `/tmp` on tmpfs. Article IV still says fail
/// loudly, but EXDEV is a known recoverable condition.
pub fn move_to_backup(cwd: &Path, run_id: Uuid, path: &Path) -> Result<PathBuf, BackupError> {
    if !path.exists() {
        return Err(BackupError::SourceMissing(path.to_path_buf()));
    }

    let op_uuid = Uuid::new_v4();
    let backup_dir = backup_root(cwd, run_id).join(op_uuid.to_string());
    std::fs::create_dir_all(&backup_dir).map_err(|e| BackupError::CreateDir {
        path: backup_dir.clone(),
        source: e,
    })?;

    // the reference `file_backup_manager.rs:29-32` falls back on a fixed name if
    // the original lacks a representable file_name — same here, but routed
    // through `unwrap_or` (allowed when the fallback is a deliberate
    // default, not a panic-shortcut; Article XIII rule 3 covers actual
    // panics, not `Option::unwrap_or`).
    let item_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown_item");
    let backup_path = backup_dir.join(item_name);

    match std::fs::rename(path, &backup_path) {
        Ok(()) => Ok(backup_path),
        Err(e) if is_cross_device(&e) => copy_then_remove(path, &backup_path),
        Err(e) => Err(BackupError::Move {
            src: path.to_path_buf(),
            dst: backup_path,
            source: e,
        }),
    }
}

/// Move a backup back to its original location. Counterpart of
/// `move_to_backup` — used by `Generator::rollback` to undo a run.
/// Same EXDEV fallback applies in reverse.
pub fn restore_from_backup(backup_path: &Path, original_path: &Path) -> Result<(), BackupError> {
    // Ensure parent exists. We can't use let-chains (workspace edition is
    // 2021); the reference codebase (see ATTRIBUTIONS.md) uses them
    // (edition 2024).
    if let Some(parent) = original_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| BackupError::CreateDir {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
    }
    match std::fs::rename(backup_path, original_path) {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device(&e) => copy_then_remove(backup_path, original_path)
            .map(|_| ())
            .map_err(|move_err| match move_err {
                BackupError::Move { source, .. } => BackupError::Restore {
                    src: backup_path.to_path_buf(),
                    dst: original_path.to_path_buf(),
                    source,
                },
                other => other,
            }),
        Err(e) => Err(BackupError::Restore {
            src: backup_path.to_path_buf(),
            dst: original_path.to_path_buf(),
            source: e,
        }),
    }
}

/// EXDEV-fallback core — copy then remove. Returns the `dst` path on
/// success or a `BackupError::Move` describing whichever step failed.
fn copy_then_remove(src: &Path, dst: &Path) -> Result<PathBuf, BackupError> {
    std::fs::copy(src, dst).map_err(|e| BackupError::Move {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        source: e,
    })?;
    std::fs::remove_file(src).map_err(|e| BackupError::Move {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        source: e,
    })?;
    Ok(dst.to_path_buf())
}

/// Detect a cross-device move failure portably. `std::io::ErrorKind::CrossesDevices`
/// is unstable on Rust 1.94 (gated behind `io_error_more`), so we match on
/// the OS error code directly:
///
/// - Linux/macOS: `EXDEV = 18`
/// - Windows: `ERROR_NOT_SAME_DEVICE = 17`
fn is_cross_device(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(17) | Some(18))
}
