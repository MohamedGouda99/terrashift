// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Agent kernel smoke test — exercises `run_agent` against a real LLM
//! via `JsonAgentLlmClient`.
//!
//! Run with:
//!
//! ```sh
//! # Configure ~/.terrashift/profile.toml first (see assets/profile.example.toml).
//! export HF_TOKEN=hf_...           # or set in your shell profile
//! cargo run --example agent_smoke -p terrashift -- --message "Hello"
//! ```
//!
//! The smoke test:
//! 1. Loads the operator profile from `~/.terrashift/profile.toml`.
//! 2. Constructs `RealClient` (via the resolver) for the eco tier.
//! 3. Wraps it in `JsonAgentLlmClient` for the agent kernel.
//! 4. Runs one bounded turn cycle with an `echo` tool the LLM can call.
//! 5. Prints the kernel's terminal state + each captured event.
//!
//! What you're verifying:
//! - The HF webhook is reachable + the env var is wired
//! - The model returns valid JSON (the kernel parses cleanly)
//! - The notifier prints the expected lifecycle events to stdout
//!
//! On parse failures, the output names what the model emitted so you
//! can iterate on the prompt or swap models without flying blind.

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use terrashift_agent_core::agent::{run_agent, AgentMessage, AgentToolDef};
use terrashift_agent_core::{
    AgentError, AgentHook, AgentLoopConfig, AgentRunContext, PassthroughCompactionEngine,
    PassthroughContextReducer, ProposedToolCall, ToolApprovalPolicy, ToolExecutionResult,
    ToolExecutor,
};
use terrashift_ai::{JsonAgentLlmClient, Profile, RealClient, Resolver, Tier};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,terrashift=debug".into()),
        )
        .init();

    let user_message = env::args()
        .skip(1)
        .skip_while(|a| a != "--message")
        .nth(1)
        .unwrap_or_else(|| "Use the echo tool to say hi, then finish.".to_string());

    println!("\n=== Terrashift agent smoke test ===\n");

    // 1. Load profile from disk.
    let profile_path = profile_path()?;
    println!("📂 Profile: {}", profile_path.display());
    let toml_str = std::fs::read_to_string(&profile_path)
        .with_context(|| format!("read profile {:?}", profile_path))?;
    let profile = Profile::from_toml(&toml_str).context("parse profile TOML")?;

    // 2. Resolve the eco-tier model and construct the real LLM client.
    let resolved = Resolver::resolve_for_tier(&profile, None, None, Tier::Eco)
        .context("resolve eco tier from profile")?;
    println!(
        "🤖 Model: {}/{} via {} (key from ${})",
        resolved.provider_key, resolved.model_id, resolved.api_endpoint, resolved.api_key_env
    );
    let real = RealClient::new(resolved).context("construct RealClient")?;

    // 3. Wrap in the JSON-mode adapter.
    let agent_llm = JsonAgentLlmClient::new(Arc::new(real), Tier::Eco);

    // 4. Set up the agent kernel.
    let executor = EchoExecutor;
    let recording = RecordingHook::default();
    let hooks: Vec<Box<dyn AgentHook>> = vec![Box::new(recording.clone())];
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let config = AgentLoopConfig {
        max_turns: 3,
        approval_policy: ToolApprovalPolicy::AcceptAll,
        ..AgentLoopConfig::default()
    };

    let ctx = AgentRunContext {
        run_id: Uuid::new_v4(),
        session_id: Uuid::new_v4(),
    };

    let tools = vec![AgentToolDef {
        name: "echo".to_string(),
        description: "Echoes its 'text' argument back as the tool result".to_string(),
        schema: json!({
            "type": "object",
            "required": ["text"],
            "properties": {"text": {"type": "string"}}
        }),
    }];

    let initial_messages = vec![
        AgentMessage::system(
            "You are a Terrashift smoke-test agent. Use tools when asked. \
            Reply only as a JSON object per the schema.",
        ),
        AgentMessage::user(&user_message),
    ];

    println!("\n👤 USER: {user_message}\n");
    println!("🚀 Running kernel (max_turns={}) ...\n", config.max_turns);

    // 5. Drive the loop.
    let result = run_agent(
        &config,
        &ctx,
        initial_messages,
        &tools,
        &executor,
        &hooks,
        &agent_llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .context("agent kernel returned an error")?;

    // 6. Print outcome + lifecycle trace.
    println!("=== Kernel terminal state ===");
    println!("reason             : {:?}", result.reason);
    println!("turns_executed     : {}", result.turns_executed);
    println!("pending_tool_calls : {:?}", result.pending_tool_calls);
    println!(
        "final_text         : {}",
        if result.final_text.is_empty() {
            "<empty>"
        } else {
            &result.final_text
        }
    );

    println!("\n=== Hook lifecycle trace ===");
    for entry in recording.snapshot() {
        println!("  {entry}");
    }

    println!("\n✅ Smoke test complete.\n");
    Ok(())
}

fn profile_path() -> Result<PathBuf> {
    if let Ok(custom) = env::var("TERRASHIFT_PROFILE") {
        return Ok(PathBuf::from(custom));
    }
    let home = dirs_home()?;
    Ok(home.join(".terrashift").join("profile.toml"))
}

fn dirs_home() -> Result<PathBuf> {
    if let Ok(p) = env::var("USERPROFILE") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(p) = env::var("HOME") {
        return Ok(PathBuf::from(p));
    }
    Err(anyhow!(
        "could not determine home directory; set TERRASHIFT_PROFILE to override"
    ))
}

// ---------------------------------------------------------------------
// In-process stubs for tools + hooks
// ---------------------------------------------------------------------

struct EchoExecutor;

#[async_trait]
impl ToolExecutor for EchoExecutor {
    async fn execute_tool_call(
        &self,
        _run: &AgentRunContext,
        tool_call: &ProposedToolCall,
        _cancel: &CancellationToken,
    ) -> Result<ToolExecutionResult, AgentError> {
        let text = tool_call
            .arguments
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("<missing 'text' argument>");
        Ok(ToolExecutionResult::Completed {
            result: format!("echoed: {text}"),
            is_error: false,
        })
    }
}

#[derive(Default, Clone)]
struct RecordingHook {
    log: Arc<std::sync::Mutex<Vec<String>>>,
}

impl RecordingHook {
    fn snapshot(&self) -> Vec<String> {
        self.log.lock().map(|v| v.clone()).unwrap_or_default()
    }

    fn push(&self, line: String) {
        if let Ok(mut v) = self.log.lock() {
            v.push(line);
        }
    }
}

#[async_trait]
impl AgentHook for RecordingHook {
    async fn before_inference(
        &self,
        run: &AgentRunContext,
        messages: &[stakai::Message],
        _model: &stakai::Model,
    ) -> Result<(), AgentError> {
        self.push(format!(
            "[before_inference] run={} messages={}",
            run.run_id,
            messages.len()
        ));
        Ok(())
    }

    async fn after_inference(
        &self,
        _run: &AgentRunContext,
        _messages: &[stakai::Message],
        _model: &stakai::Model,
    ) -> Result<(), AgentError> {
        self.push("[after_inference]".to_string());
        Ok(())
    }

    async fn before_tool_execution(
        &self,
        _run: &AgentRunContext,
        tc: &ProposedToolCall,
        _messages: &[stakai::Message],
    ) -> Result<(), AgentError> {
        self.push(format!("[before_tool] id={} name={}", tc.id, tc.name));
        Ok(())
    }

    async fn after_tool_execution(
        &self,
        _run: &AgentRunContext,
        tc: &ProposedToolCall,
        _messages: &[stakai::Message],
    ) -> Result<(), AgentError> {
        self.push(format!("[after_tool] id={} name={}", tc.id, tc.name));
        Ok(())
    }
}
