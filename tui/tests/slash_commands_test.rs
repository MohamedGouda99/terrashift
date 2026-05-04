//! Integration tests for the P-14 slash-command registry.
//!
//! Pattern: per-file commands from refs/claude-code/src/commands/.

use terrashift_tui::commands::{Action, CommandOutcome, Registry};

// ─────────────────────────────────────────────────────────────────────────
// Test 1 — Registry has 11 entries (Stage 1 = 8; +scan +quit + /exit alias).
//
// /exit and /quit map to the same `Quit` handler so `list()` shows "quit"
// twice — that's expected. Dispatch works for both keys (test below).
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn registry_has_eleven_commands() {
    let r = Registry::stage1();
    assert_eq!(
        r.len(),
        11,
        "Stage 1 ships 11 slash commands (8 P-14 + /scan + /quit + /exit alias)"
    );

    // Dispatch by name verifies all commands are reachable, including
    // the /exit alias that wouldn't show distinctly in `list()`.
    use terrashift_tui::commands::{Action, CommandOutcome};
    for name in ["help", "audit", "migrate", "plan", "cost", "scan"] {
        let out = r.dispatch(&format!("/{name}"));
        assert!(
            !matches!(out, CommandOutcome::Error(ref s) if s.contains("unknown command")),
            "/{name} should be reachable; got {:?}",
            out
        );
    }
    // /exit and /quit both produce Action::Exit.
    for name in ["quit", "exit"] {
        match r.dispatch(&format!("/{name}")) {
            CommandOutcome::Action(Action::Exit) => {}
            other => panic!("/{name} should return Action::Exit; got {:?}", other),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 2 — /help lists every command (criterion #2)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn help_lists_every_command() {
    let r = Registry::stage1();
    let outcome = r.dispatch("/help");
    let text = match outcome {
        CommandOutcome::Text(t) => t,
        other => panic!("expected Text from /help, got {:?}", other),
    };
    for expected in [
        "/help",
        "/audit",
        "/migrate",
        "/checkpoint",
        "/plan",
        "/cost",
        "/rollback",
        "/compact",
    ] {
        assert!(
            text.contains(expected),
            "/help output missing '{expected}'. Got:\n{text}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 3 — Unknown command friendly error (criterion #3, Article IV)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn unknown_command_returns_friendly_error() {
    let r = Registry::stage1();
    let outcome = r.dispatch("/nonexistent");
    match outcome {
        CommandOutcome::Error(msg) => {
            assert!(
                msg.contains("unknown command"),
                "msg should say 'unknown command': {msg}"
            );
            assert!(
                msg.contains("nonexistent"),
                "msg should name the bad command: {msg}"
            );
            assert!(
                msg.contains("/help"),
                "msg should point users at /help: {msg}"
            );
        }
        other => panic!("expected Error, got {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 4 — Metadata stability (criterion #4)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn every_command_has_nonempty_metadata() {
    let r = Registry::stage1();
    for (name, desc) in r.list() {
        assert!(!name.is_empty(), "command name must be non-empty");
        assert!(
            !desc.is_empty(),
            "command /{name}'s description must be non-empty"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 5 — Dispatch parses args (criterion #5)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn migrate_parses_path_argument() {
    let r = Registry::stage1();
    let outcome = r.dispatch("/migrate path/to/plan.json");
    match outcome {
        CommandOutcome::Action(Action::StartMigration { plan_path }) => {
            assert_eq!(plan_path, "path/to/plan.json");
        }
        other => panic!("expected StartMigration action, got {:?}", other),
    }
}

#[test]
fn migrate_without_path_errors() {
    let r = Registry::stage1();
    match r.dispatch("/migrate") {
        CommandOutcome::Error(msg) => assert!(msg.contains("plan path")),
        other => panic!("expected Error for no-arg /migrate, got {:?}", other),
    }
}

#[test]
fn rollback_parses_run_id() {
    let r = Registry::stage1();
    let outcome = r.dispatch("/rollback abc-123-def");
    match outcome {
        CommandOutcome::Action(Action::Rollback { run_id }) => {
            assert_eq!(run_id, "abc-123-def");
        }
        other => panic!("expected Rollback action, got {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 6 — Empty input
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn empty_input_errors() {
    let r = Registry::stage1();
    match r.dispatch("") {
        CommandOutcome::Error(msg) => assert!(msg.contains("empty")),
        other => panic!("expected Error for empty input, got {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Test 7 — Leading slash is optional
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn dispatch_accepts_input_without_leading_slash() {
    let r = Registry::stage1();
    let with_slash = r.dispatch("/help");
    let without = r.dispatch("help");
    assert_eq!(with_slash, without);
}

// ─────────────────────────────────────────────────────────────────────────
// Test 8 — Action variants for /checkpoint and /compact.
// IMPORTANT: `matches!` returns a bool that must be passed to `assert!` —
// using it bare evaluates and discards, making the test vacuously pass.
// (Self-review caught this; fixed before commit.)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn checkpoint_emits_action() {
    let r = Registry::stage1();
    assert!(matches!(
        r.dispatch("/checkpoint"),
        CommandOutcome::Action(Action::Checkpoint)
    ));
}

#[test]
fn compact_emits_action() {
    let r = Registry::stage1();
    assert!(matches!(
        r.dispatch("/compact"),
        CommandOutcome::Action(Action::Compact)
    ));
}

// ─────────────────────────────────────────────────────────────────────────
// Test 9 — Stub commands return Text (Stage 2+ "not implemented" notice)
// ─────────────────────────────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────
// Test — /scan with no arg → Error; with valid path → Text containing
// resource type lines. Mirrors the CLI's `terrashift scan` UX.
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn scan_without_arg_errors() {
    let r = Registry::stage1();
    match r.dispatch("/scan") {
        CommandOutcome::Error(msg) => assert!(
            msg.contains("requires a directory") || msg.contains("path"),
            "scan error should mention directory: {msg}"
        ),
        other => panic!("expected Error for no-arg /scan, got {:?}", other),
    }
}

#[test]
fn scan_with_nonexistent_path_errors() {
    let r = Registry::stage1();
    match r.dispatch("/scan C:/this/path/does/not/exist/anywhere") {
        CommandOutcome::Error(msg) => assert!(
            msg.contains("does not exist") || msg.contains("scan failed"),
            "scan error should explain why: {msg}"
        ),
        other => panic!("expected Error for bad path, got {:?}", other),
    }
}

#[test]
fn scan_against_inline_fixture_returns_text() {
    use std::io::Write;
    let r = Registry::stage1();
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let mut f = std::fs::File::create(tmp.path().join("inline.tf")).expect("create");
    writeln!(
        f,
        r#"resource "aws_vpc" "main" {{ cidr_block = "10.0.0.0/16" }}"#
    )
    .expect("write");
    drop(f);

    let cmd = format!("/scan {}", tmp.path().display());
    match r.dispatch(&cmd) {
        CommandOutcome::Text(txt) => {
            assert!(
                txt.contains("aws_vpc"),
                "scan output should list aws_vpc: {txt}"
            );
            assert!(
                txt.contains("1 resource") || txt.contains("1 file"),
                "scan output should report counts: {txt}"
            );
        }
        other => panic!("expected Text from /scan, got {:?}", other),
    }
}

#[test]
fn plan_and_cost_stubs_return_text() {
    let r = Registry::stage1();
    for cmd in ["/plan", "/cost"] {
        match r.dispatch(cmd) {
            CommandOutcome::Text(t) => assert!(
                t.contains("Stage 2+") || t.contains("not implemented"),
                "stub should mention deferral: {cmd} → {t}"
            ),
            other => panic!("expected Text from stub {cmd}, got {:?}", other),
        }
    }
}
