// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! End-to-end CLI smoke tests — drive the actual `terrashift` binary as a
//! subprocess and assert exit codes + output. These run in CI on every push.
//!
//! Why subprocess and not library calls? The point is to catch the class of
//! bugs where the library is fine but the binary glue (clap config, error
//! formatting, exit codes) is wrong. A unit test on `commands::scan::run`
//! would have missed the "anyhow Debug prints a 30-line backtrace" bug we
//! shipped on `main` until we tested the actual binary.

use std::process::Command;

/// Cargo populates `CARGO_BIN_EXE_<binname>` with the absolute path to the
/// freshly-compiled `[[bin]]` target before invoking the test harness.
fn ts() -> Command {
    Command::new(env!("CARGO_BIN_EXE_terrashift"))
}

#[test]
fn version_succeeds() {
    let out = ts().arg("version").output().expect("spawn");
    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("terrashift"), "stdout: {stdout}");
}

#[test]
fn help_lists_all_subcommands() {
    let out = ts().arg("--help").output().expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in ["version", "migrate", "schema", "scan"] {
        assert!(
            stdout.contains(sub),
            "missing subcommand `{sub}` in --help: {stdout}"
        );
    }
}

#[test]
fn migrate_without_args_returns_clap_error() {
    let out = ts().arg("migrate").output().expect("spawn");
    // Clap exits 2 on bad args.
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--source"), "stderr: {stderr}");
    assert!(stderr.contains("--from"));
    assert!(stderr.contains("--to"));
}

#[test]
fn unknown_subcommand_returns_clap_error() {
    let out = ts()
        .arg("definitely-not-a-command")
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unrecognized"), "stderr: {stderr}");
}

#[test]
fn scan_missing_arg_returns_clap_error() {
    let out = ts().arg("scan").output().expect("spawn");
    assert_eq!(out.status.code(), Some(2));
}

/// Regression test: a recoverable error should NOT dump a Rust stack
/// backtrace to the user. We caught this by testing the binary directly.
#[test]
fn scan_nonexistent_dir_prints_clean_error_no_backtrace() {
    // Construct a path inside a fresh tempdir that we deliberately never
    // create. Cross-platform: tempdir resolves to the OS-appropriate root,
    // and the join is just path arithmetic — no OS-specific shape.
    let dir = tempfile::tempdir().expect("tempdir");
    let nonexistent = dir.path().join("definitely-not-a-real-subdir");
    let out = ts().arg("scan").arg(&nonexistent).output().expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1 for runtime error"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    // Display chain should be present.
    assert!(stderr.contains("error:"), "stderr: {stderr}");
    assert!(stderr.contains("does not exist"), "stderr: {stderr}");
    // No Rust backtrace leakage.
    assert!(
        !stderr.contains("Stack backtrace"),
        "backtrace should not be shown to users: {stderr}"
    );
    assert!(
        !stderr.contains("at /rustc/"),
        "rustc internals should not be shown to users: {stderr}"
    );
}

#[test]
fn scan_real_fixture_succeeds() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("fixtures")
        .join("aws-to-azure-real")
        .join("aws")
        .join("modules")
        .join("vpc");
    if !fixture.exists() {
        // Don't hard-fail when running in environments without the fixture.
        eprintln!("skipping: fixture {fixture:?} missing");
        return;
    }

    let out = ts().arg("scan").arg(&fixture).output().expect("spawn");
    assert!(
        out.status.success(),
        "exit: {:?} stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("aws_vpc"), "expected aws_vpc in: {stdout}");
}

#[test]
fn schema_list_with_no_cache_succeeds() {
    // Isolated cache root: regardless of what's in the user's real
    // ~/.terrashift cache, list against an empty tempdir is well-defined.
    let dir = tempfile::tempdir().expect("tempdir");
    let out = ts()
        .arg("schema")
        .arg("list")
        .arg("--cache")
        .arg(dir.path())
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "exit: {:?} stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn schema_help_lists_all_five_subcommands() {
    let out = ts().arg("schema").arg("--help").output().expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in ["list", "update", "show", "verify", "gc"] {
        assert!(stdout.contains(sub), "missing `{sub}` in: {stdout}");
    }
}

#[test]
fn schema_update_without_provider_returns_clap_error() {
    // Without --provider/--version (and no --all), update must surface a
    // clap error (exit 2), not run terraform with empty arguments.
    let dir = tempfile::tempdir().expect("tempdir");
    let out = ts()
        .arg("schema")
        .arg("update")
        .arg("--cache")
        .arg(dir.path())
        .output()
        .expect("spawn");
    // Either clap rejects (exit 2) or our anyhow validation does (exit 1).
    // Both are acceptable loud failures (Article IV).
    assert!(
        out.status.code() == Some(1) || out.status.code() == Some(2),
        "expected exit 1 or 2, got {:?}: stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn schema_show_with_invalid_spec_returns_clean_error() {
    // Invalid `<provider>@<version>` spec must produce a clear error,
    // not a panic or silent failure.
    let dir = tempfile::tempdir().expect("tempdir");
    let out = ts()
        .arg("schema")
        .arg("show")
        .arg("not-a-spec")
        .arg("--cache")
        .arg(dir.path())
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(1), "expected exit 1 for bad spec");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error:"), "stderr should be loud: {stderr}");
    // No Rust backtrace leakage (regression test pattern from line 91).
    assert!(
        !stderr.contains("Stack backtrace"),
        "no rustc backtrace: {stderr}"
    );
}

#[test]
fn schema_verify_with_empty_cache_succeeds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = ts()
        .arg("schema")
        .arg("verify")
        .arg("--cache")
        .arg(dir.path())
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "verify against empty cache must exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn schema_gc_dry_run_does_not_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    // No manifest.json — gc against empty cache exits 0, prints "nothing to gc".
    let out = ts()
        .arg("schema")
        .arg("gc")
        .arg("--dry-run")
        .arg("--cache")
        .arg(dir.path())
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "dry-run gc must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
