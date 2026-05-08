// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Subprocess invocation — `LocalRunner` (direct exec) and
//! `DockerRunner` (wraps in `docker run hashicorp/terraform:...`).
//!
//! Pattern: spec `specs/s5-close-executor/spec.md` US3 + plan §runner.
//! the architecture reference §15 (executor pattern) + §29 (Warden sandbox).
//!
//! ## Why a trait
//!
//! Two impls today (Local + Docker) and tests use a stub. The trait
//! lets `Executor::apply` swap implementations at construction time
//! without coupling business logic (gate, audit) to subprocess
//! mechanics.
//!
//! ## Subprocess shape
//!
//! Commands are passed as `Vec<String>` of (program, args...). The
//! runner spawns directly via `tokio::process::Command::new(program)
//! .args(args)` — no shell intermediary, no shell-metacharacter
//! interpretation. This is intentional: terraform commands don't need
//! shell features, and avoiding the shell removes an injection
//! surface (Article V).
//!
//! ## Streaming + tee
//!
//! stdout/stderr are streamed line-by-line to the operator's stdout
//! AND captured to a per-command log file at
//! `<cwd>/.terrashift/runs/<run_id>/logs/<n>-<command>.log`. The log
//! file is post-execution scanned for secret leakage by the caller
//! (`Executor::apply`).
//!
//! Constitution: Article V (sandboxed apply, command-level approval),
//! Article XIII rule 7 (env vars set on subprocess only — never to disk).

use crate::executor::errors::ExecutorError;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Result of one successful subprocess run.
#[derive(Debug, Clone)]
pub struct SubprocessOutcome {
    /// The program that was spawned (post argv[0] resolution).
    pub program: String,
    /// All arguments after argv[0].
    pub args: Vec<String>,
    /// Process exit code. `0` for success; `terraform plan` returns
    /// `2` on "changes detected" — caller decides what's acceptable.
    pub exit_code: i32,
    /// Wall-clock duration.
    pub duration_ms: u128,
    /// Path to the captured log file. Caller post-scans for secret leakage.
    pub log_path: PathBuf,
}

/// Subprocess invocation contract. Implementations handle process
/// lifecycle, env injection, output streaming, and log file capture.
#[async_trait]
pub trait SubprocessRunner: Send + Sync {
    /// Run a command. `command[0]` is the program; rest are args.
    /// `env` is appended to the subprocess's environment (does not
    /// inherit the caller's full env unless the runner explicitly
    /// chooses to).
    /// `cwd` is the working directory.
    /// `log_path` is where stdout+stderr are tee'd.
    async fn run(
        &self,
        command: &[String],
        env: &[(String, String)],
        cwd: &Path,
        log_path: &Path,
    ) -> Result<SubprocessOutcome, ExecutorError>;
}

/// Direct subprocess execution — runs the program on the host
/// machine with `--no-sandbox`. Inherits parent env then overlays
/// `env` argument.
pub struct LocalRunner;

#[async_trait]
impl SubprocessRunner for LocalRunner {
    async fn run(
        &self,
        command: &[String],
        env: &[(String, String)],
        cwd: &Path,
        log_path: &Path,
    ) -> Result<SubprocessOutcome, ExecutorError> {
        if command.is_empty() {
            return Err(ExecutorError::EmptyCommand);
        }
        let program = command[0].clone();
        let args: Vec<String> = command[1..].to_vec();

        if let Some(parent) = log_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ExecutorError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
        }

        let mut cmd = Command::new(&program);
        cmd.args(&args);
        cmd.current_dir(cwd);
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let started = Instant::now();
        let mut child: Child = cmd.spawn().map_err(|e| ExecutorError::SpawnFailed {
            program: program.clone(),
            source: e,
        })?;

        let log_file = tokio::fs::File::create(log_path)
            .await
            .map_err(|e| ExecutorError::Io {
                path: log_path.to_path_buf(),
                source: e,
            })?;
        let log_file = std::sync::Arc::new(tokio::sync::Mutex::new(log_file));

        let stdout = child
            .stdout
            .take()
            .ok_or(ExecutorError::PipeUnavailable { stream: "stdout" })?;
        let stderr = child
            .stderr
            .take()
            .ok_or(ExecutorError::PipeUnavailable { stream: "stderr" })?;

        let log_out = log_file.clone();
        let stdout_handle = tokio::spawn(async move {
            stream_lines(stdout, "stdout", log_out).await;
        });
        let log_err = log_file.clone();
        let stderr_handle = tokio::spawn(async move {
            stream_lines(stderr, "stderr", log_err).await;
        });

        let status = child.wait().await.map_err(|e| ExecutorError::SpawnFailed {
            program: program.clone(),
            source: e,
        })?;

        let _ = stdout_handle.await;
        let _ = stderr_handle.await;

        Ok(SubprocessOutcome {
            program,
            args,
            exit_code: status.code().unwrap_or(-1),
            duration_ms: started.elapsed().as_millis(),
            log_path: log_path.to_path_buf(),
        })
    }
}

/// Docker-isolated subprocess. Builds:
/// `docker run --rm -v <cwd>:/work -w /work -e K=V... <image> <command...>`.
///
/// Default image: `hashicorp/terraform:1.10`. Override via
/// `DockerRunner::with_image`.
pub struct DockerRunner {
    image: String,
}

impl DockerRunner {
    pub fn new() -> Self {
        Self {
            image: "hashicorp/terraform:1.10".to_string(),
        }
    }

    pub fn with_image(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
        }
    }

    /// Construct the wrapped `docker run ...` command array. Exposed
    /// for tests so they can assert arg shape without spawning Docker.
    pub fn build_docker_args(
        &self,
        command: &[String],
        env: &[(String, String)],
        cwd: &Path,
    ) -> Vec<String> {
        let mut docker_args: Vec<String> = vec![
            "run".to_string(),
            "--rm".to_string(),
            "-v".to_string(),
            format!("{}:/work", cwd.display()),
            "-w".to_string(),
            "/work".to_string(),
        ];
        for (k, v) in env {
            docker_args.push("-e".to_string());
            docker_args.push(format!("{}={}", k, v));
        }
        docker_args.push(self.image.clone());
        for c in command {
            docker_args.push(c.clone());
        }
        docker_args
    }
}

impl Default for DockerRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SubprocessRunner for DockerRunner {
    async fn run(
        &self,
        command: &[String],
        env: &[(String, String)],
        cwd: &Path,
        log_path: &Path,
    ) -> Result<SubprocessOutcome, ExecutorError> {
        // Docker is just a wrapper — assemble args, then delegate to
        // LocalRunner with `docker` as the program. This keeps streaming
        // + log capture logic in one place.
        let mut full_command: Vec<String> = vec!["docker".to_string()];
        full_command.extend(self.build_docker_args(command, env, cwd));
        // The env vars are already passed as `-e K=V` to docker; clear
        // for the outer Command so they don't leak into the docker CLI itself.
        let outer_env: Vec<(String, String)> = Vec::new();
        LocalRunner
            .run(&full_command, &outer_env, cwd, log_path)
            .await
    }
}

async fn stream_lines<R>(
    stream: R,
    label: &'static str,
    log_file: std::sync::Arc<tokio::sync::Mutex<tokio::fs::File>>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        // Operator-visible stream — print to console with label.
        println!("[{label}] {line}");
        // Tee to log file (label included for secret-scan post-pass).
        let mut file = log_file.lock().await;
        let _ = file.write_all(line.as_bytes()).await;
        let _ = file.write_all(b"\n").await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn local_runner_spawns_echo_and_captures_output() {
        let cwd = TempDir::new().unwrap();
        let log = cwd.path().join("test.log");

        // Use a portable echo command — `cmd /c echo` on Windows, `echo` on Unix.
        #[cfg(windows)]
        let command: Vec<String> = vec![
            "cmd".to_string(),
            "/c".to_string(),
            "echo".to_string(),
            "hello".to_string(),
        ];
        #[cfg(not(windows))]
        let command: Vec<String> = vec!["echo".to_string(), "hello".to_string()];

        let outcome = LocalRunner
            .run(&command, &[], cwd.path(), &log)
            .await
            .unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert!(log.exists(), "log file should be created");
        let log_contents = std::fs::read_to_string(&log).unwrap();
        assert!(
            log_contents.contains("hello"),
            "captured log should contain 'hello': {log_contents}"
        );
    }

    #[test]
    fn docker_runner_builds_canonical_args() {
        let cwd = std::path::PathBuf::from("/host/work");
        let command: Vec<String> = vec!["terraform".to_string(), "plan".to_string()];
        let env: Vec<(String, String)> = vec![
            ("AWS_REGION".to_string(), "us-east-1".to_string()),
            ("AWS_ACCESS_KEY_ID".to_string(), "AKIA...".to_string()),
        ];
        let runner = DockerRunner::new();
        let args = runner.build_docker_args(&command, &env, &cwd);

        assert_eq!(args[0], "run");
        assert_eq!(args[1], "--rm");
        assert!(args.iter().any(|a| a == "-v"), "should bind-mount cwd");
        assert!(
            args.iter().any(|a| a == "AWS_REGION=us-east-1"),
            "should pass env vars via -e"
        );
        assert!(
            args.iter().any(|a| a.contains("hashicorp/terraform")),
            "should reference terraform image"
        );
        // Command appears AFTER image
        let image_idx = args.iter().position(|a| a.contains("terraform:")).unwrap();
        assert_eq!(args[image_idx + 1], "terraform");
        assert_eq!(args[image_idx + 2], "plan");
    }

    #[test]
    fn docker_runner_image_override() {
        let runner = DockerRunner::with_image("my-registry/terraform:1.5");
        let args = runner.build_docker_args(&[], &[], &PathBuf::from("/x"));
        assert!(args.iter().any(|a| a == "my-registry/terraform:1.5"));
    }

    #[tokio::test]
    async fn local_runner_empty_command_is_loud_error() {
        let cwd = TempDir::new().unwrap();
        let log = cwd.path().join("empty.log");
        let result = LocalRunner.run(&[], &[], cwd.path(), &log).await;
        assert!(matches!(result, Err(ExecutorError::EmptyCommand)));
    }

    #[tokio::test]
    async fn local_runner_unknown_program_is_loud_error() {
        let cwd = TempDir::new().unwrap();
        let log = cwd.path().join("unknown.log");
        let result = LocalRunner
            .run(
                &["this-program-does-not-exist-9999".to_string()],
                &[],
                cwd.path(),
                &log,
            )
            .await;
        assert!(matches!(result, Err(ExecutorError::SpawnFailed { .. })));
    }
}
