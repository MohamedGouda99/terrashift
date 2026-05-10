// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Asserts the CLI does not emit ANSI escape sequences when stdout is
//! piped (non-TTY). Screen readers and log aggregators choke on raw
//! ANSI; this test catches accidental colour-on-everywhere regressions.
//! Spec: docs/superpowers/specs/2026-05-10-extended-testing-design.md §5.3.

use assert_cmd::Command;

fn assert_no_ansi(output: &[u8]) {
    let s = String::from_utf8_lossy(output);
    assert!(
        !s.contains('\x1b'),
        "ANSI escape sequence (ESC, 0x1B) found in stdout — non-TTY output must be plain ASCII\n--- output ---\n{s}\n",
    );
}

#[test]
fn version_command_is_plain_when_piped() {
    let out = Command::cargo_bin("terrashift")
        .expect("locate cargo bin")
        .arg("version")
        .output()
        .expect("run terrashift version");
    assert_no_ansi(&out.stdout);
    assert_no_ansi(&out.stderr);
}

#[test]
fn help_text_is_plain_when_piped() {
    let out = Command::cargo_bin("terrashift")
        .expect("locate cargo bin")
        .arg("--help")
        .output()
        .expect("run terrashift --help");
    assert_no_ansi(&out.stdout);
    assert_no_ansi(&out.stderr);
}

#[test]
fn schema_list_is_plain_when_piped() {
    let out = Command::cargo_bin("terrashift")
        .expect("locate cargo bin")
        .arg("schema")
        .arg("list")
        .output()
        .expect("run terrashift schema list");
    // schema list may exit non-zero (no profile configured) — that's OK,
    // we're only checking stdout/stderr cleanliness.
    assert_no_ansi(&out.stdout);
    assert_no_ansi(&out.stderr);
}
