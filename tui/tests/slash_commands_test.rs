//! Integration tests for the P-14 slash-command registry.
//!
//! Pattern: per-file commands from refs/claude-code/src/commands/.

use terrashift_tui::commands::{Action, CommandOutcome, Registry};

// ─────────────────────────────────────────────────────────────────────────
// Test 1 — Registry has 8 commands (criterion #1)
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn registry_has_eight_commands() {
    let r = Registry::stage1();
    assert_eq!(r.len(), 8, "Stage 1 ships exactly 8 slash commands");
    let names: Vec<&str> = r.list().iter().map(|(n, _)| *n).collect();
    for expected in [
        "help",
        "audit",
        "migrate",
        "checkpoint",
        "plan",
        "cost",
        "rollback",
        "compact",
    ] {
        assert!(
            names.contains(&expected),
            "registry must contain /{expected}; got {:?}",
            names
        );
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
