// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `terrashift feedback` — open a pre-filled GitHub issue.
//!
//! Pattern: thin CLI / fat libs (Article VII).
//! Failure mode: if the browser can't be launched, the URL is printed
//! to stdout — never silently swallowed (Article IV).

use anyhow::Result;
use terrashift_feedback::{FeedbackCategory, FeedbackReport};

const REPO_URL: &str = "https://github.com/MohamedGouda99/terrashift";

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Category of feedback.
    #[arg(long, value_enum)]
    pub category: Category,

    /// Free-text message body.
    #[arg(long)]
    pub message: String,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum Category {
    Bug,
    Idea,
    Other,
}

impl From<Category> for FeedbackCategory {
    fn from(c: Category) -> Self {
        match c {
            Category::Bug => FeedbackCategory::Bug,
            Category::Idea => FeedbackCategory::Idea,
            Category::Other => FeedbackCategory::Other,
        }
    }
}

pub fn run(args: Args) -> Result<()> {
    if args.message.trim().is_empty() {
        anyhow::bail!("--message must not be empty");
    }
    let report = FeedbackReport {
        category: args.category.into(),
        message: args.message,
        run_id: None,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let url = report.to_github_issue_url(REPO_URL);

    match webbrowser::open(&url) {
        Ok(()) => {
            println!("✓ Opening feedback form in your browser.");
            println!("  If it didn't open, paste this URL: {url}");
        }
        Err(e) => {
            eprintln!("Could not launch browser ({e}).");
            println!("→ Open this URL in your browser:");
            println!("  {url}");
        }
    }
    Ok(())
}
