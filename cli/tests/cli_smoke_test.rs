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
    for sub in ["version", "migrate", "schemas", "scan"] {
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
    let out = ts()
        .arg("scan")
        .arg("C:/this/path/definitely/does/not/exist")
        .output()
        .expect("spawn");
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
fn schemas_list_with_no_cache_succeeds() {
    // We can't easily isolate the user's real ~/.terrashift cache, but the
    // command must not panic regardless of cache state. Either "Cache empty"
    // or a list of (provider, version) tuples is acceptable.
    let out = ts().arg("schemas").arg("list").output().expect("spawn");
    assert!(
        out.status.success(),
        "exit: {:?} stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn schemas_help_lists_subcommands() {
    let out = ts().arg("schemas").arg("--help").output().expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in ["sync", "list", "seed"] {
        assert!(stdout.contains(sub), "missing `{sub}` in: {stdout}");
    }
}
