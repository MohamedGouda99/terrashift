// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `cargo xtask audit-citations` — verify every Constitution article is
//! cited in at least one merged or open PR.
//!
//! Closes MVP gate criterion #5 ("all 13 constitution articles cited in
//! at least one PR" — issue #10).
//!
//! ## Why no hardcoded article list
//!
//! Per Article XI, the constitution can be amended (new articles, new
//! Article XIII rules). If we hardcode `let articles = ["I"..."XIII"]`,
//! the audit silently misses Article XIV the day it's added. Instead we
//! read `docs/governance/CONSTITUTION.md` at runtime, regex-extract every
//! `## Article <roman>` heading, and use that as the canonical list.
//!
//! ## Workflow
//!
//! 1. Read CONSTITUTION.md, parse `## Article <roman>` headings.
//! 2. Run `gh pr list --state all --limit 200 --json number,title,body,state,mergedAt`.
//! 3. For each PR, find every `Article <roman>` substring and increment
//!    the per-article hit counter.
//! 4. Write `docs/governance/article-citation-audit.md` with a table of
//!    (article, hit count, latest PR, status).
//! 5. Exit 1 if any article has zero hits across all PRs (Article IV —
//!    silent failure modes are a CI failure).

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Skip the `gh` CLI call and read PR JSON from this file instead.
    /// Useful for offline tests + reproducible CI.
    #[arg(long)]
    pub fixture: Option<PathBuf>,

    /// Path to CONSTITUTION.md. Default: `docs/governance/CONSTITUTION.md`
    /// relative to the repo root.
    #[arg(long, default_value = "docs/governance/CONSTITUTION.md")]
    pub constitution: PathBuf,

    /// Where to write the audit report. Default:
    /// `docs/governance/article-citation-audit.md`.
    #[arg(long, default_value = "docs/governance/article-citation-audit.md")]
    pub output: PathBuf,

    /// `--check` exits non-zero if any article has zero citations (CI mode).
    /// Default: prints + writes report regardless of gaps.
    #[arg(long, default_value_t = false)]
    pub check: bool,
}

#[derive(Debug, Deserialize)]
struct PullRequest {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    /// Carried for forward-compat (future report can show merge timestamps);
    /// not exercised by the Stage-1 surface.
    #[serde(default)]
    #[allow(dead_code)]
    merged_at: Option<String>,
}

#[derive(Debug, Default)]
struct ArticleStats {
    hit_count: usize,
    latest_pr: Option<u64>,
    latest_pr_title: Option<String>,
    latest_pr_state: Option<String>,
}

pub async fn run(args: Args) -> Result<()> {
    // 1. Load + parse the canonical article list.
    let articles = load_canonical_articles(&args.constitution)
        .with_context(|| format!("read articles from {}", args.constitution.display()))?;
    if articles.is_empty() {
        return Err(anyhow!(
            "no '## Article <roman>' headings found in {} — has the constitution been moved or restructured?",
            args.constitution.display()
        ));
    }
    tracing::info!(
        article_count = articles.len(),
        "loaded canonical article list"
    );

    // 2. Load PR list (from fixture or live `gh`).
    let prs = match args.fixture.as_ref() {
        Some(path) => load_prs_from_fixture(path)
            .with_context(|| format!("read PR fixture {}", path.display()))?,
        None => {
            load_prs_via_gh().context("fetch PRs via gh — run `gh auth login` if you haven't")?
        }
    };
    tracing::info!(pr_count = prs.len(), "loaded PR list");

    // 3. Tally citations.
    let mut stats: BTreeMap<String, ArticleStats> = articles
        .iter()
        .map(|a| (a.clone(), ArticleStats::default()))
        .collect();

    for pr in &prs {
        let Some(body) = pr.body.as_deref() else {
            continue;
        };
        for article in &articles {
            if body_cites_article(body, article) {
                let entry = stats.entry(article.clone()).or_default();
                entry.hit_count += 1;
                // PRs come back ordered newest-first by gh default, so
                // the first match is the latest.
                if entry.latest_pr.is_none() {
                    entry.latest_pr = Some(pr.number);
                    entry.latest_pr_title = Some(pr.title.clone());
                    entry.latest_pr_state = Some(pr.state.clone());
                }
            }
        }
    }

    // 4. Write the report.
    let report = render_report(&articles, &stats, &prs);
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&args.output, &report)
        .with_context(|| format!("write report to {}", args.output.display()))?;
    println!("✓ Wrote citation audit to {}", args.output.display());

    // 5. Gate: any zero-citation article fails in --check mode.
    let zero_articles: Vec<&String> = articles
        .iter()
        .filter(|a| stats.get(*a).map(|s| s.hit_count).unwrap_or(0) == 0)
        .collect();
    if !zero_articles.is_empty() {
        eprintln!();
        eprintln!("⚠ Articles with zero citations across {} PRs:", prs.len());
        for a in &zero_articles {
            eprintln!("  - Article {a}");
        }
        if args.check {
            return Err(anyhow!(
                "{} article(s) have no citations — see {} for full report",
                zero_articles.len(),
                args.output.display()
            ));
        }
    } else {
        println!(
            "✓ All {} articles have at least one citation across {} PRs",
            articles.len(),
            prs.len()
        );
    }

    Ok(())
}

/// Parse `## Article <roman>` headings from CONSTITUTION.md.
///
/// We match lines that start with `## Article ` and end with a roman-
/// numeral token (no extra body text on the line). The roman numeral is
/// then validated against the strict 1..=99 grammar — anything else
/// (e.g., section dividers like "## Article XIII — Rules") is also
/// accepted; we strip everything after the first whitespace following
/// "Article ".
fn load_canonical_articles(path: &std::path::Path) -> Result<Vec<String>> {
    let raw = std::fs::read_to_string(path)?;
    let mut articles = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for line in raw.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("## Article ") else {
            continue;
        };
        // Take chars up to the first whitespace — that's the roman numeral.
        let token: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace())
            .collect::<String>()
            .trim_end_matches([',', '.', ':', ';', '—'])
            .to_string();
        if is_valid_roman(&token) && seen.insert(token.clone()) {
            articles.push(token);
        }
    }
    Ok(articles)
}

/// Strict roman-numeral validity for the 1..=99 range we expect for a
/// constitution. Reject anything that doesn't decompose into the standard
/// subtractive form (avoids accepting "VIIII" or "IIII" as IX or IV).
fn is_valid_roman(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let valid_chars = s.chars().all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C'));
    if !valid_chars {
        return false;
    }
    if let Some(n) = roman_to_u32(s) {
        // Round-trip: re-render and compare to filter invalid forms like "IIII".
        u32_to_roman(n) == s
    } else {
        false
    }
}

fn roman_to_u32(s: &str) -> Option<u32> {
    let mut total: i32 = 0;
    let mut prev: i32 = 0;
    for c in s.chars().rev() {
        let v = match c {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            'C' => 100,
            _ => return None,
        };
        if v < prev {
            total -= v;
        } else {
            total += v;
            prev = v;
        }
    }
    if total > 0 {
        Some(total as u32)
    } else {
        None
    }
}

fn u32_to_roman(n: u32) -> String {
    let pairs = [
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    let mut remaining = n;
    for (val, sym) in pairs {
        while remaining >= val {
            out.push_str(sym);
            remaining -= val;
        }
    }
    out
}

/// Find a citation of `Article <roman>` in `body`, with a word boundary
/// after the roman numeral.
///
/// Iterates through ALL occurrences of the substring "Article <article>"
/// because a body can mix "Article XIII" (which begins with "Article XI"
/// which begins with "Article X" which begins with "Article I"). A naive
/// single-find would land on the first textual occurrence which may be
/// inside a longer roman numeral, then fail the word-boundary check —
/// returning false even though a real citation exists later in the body.
fn body_cites_article(body: &str, article: &str) -> bool {
    let needle = format!("Article {article}");
    let mut search_start = 0usize;
    while let Some(rel_idx) = body.get(search_start..).and_then(|s| s.find(&needle)) {
        let abs_idx = search_start + rel_idx;
        let tail_start = abs_idx + needle.len();
        let after = body.get(tail_start..).and_then(|s| s.chars().next());
        let word_boundary = match after {
            None => true,
            Some(c) => !matches!(c, 'I' | 'V' | 'X' | 'L' | 'C'),
        };
        if word_boundary {
            return true;
        }
        // Skip past this match's first character and keep looking — the
        // current occurrence is inside a longer roman numeral, but a
        // real citation may follow.
        search_start = abs_idx + 1;
    }
    false
}

fn load_prs_via_gh() -> Result<Vec<PullRequest>> {
    let out = Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "200",
            "--json",
            "number,title,body,state,mergedAt",
        ])
        .output()
        .context("invoke `gh pr list`")?;
    if !out.status.success() {
        return Err(anyhow!(
            "`gh pr list` failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let prs: Vec<PullRequest> = serde_json::from_slice(&out.stdout)
        .context("parse PR JSON from gh — expected a JSON array")?;
    Ok(prs)
}

fn load_prs_from_fixture(path: &std::path::Path) -> Result<Vec<PullRequest>> {
    let raw = std::fs::read_to_string(path)?;
    let prs: Vec<PullRequest> = serde_json::from_str(&raw)?;
    Ok(prs)
}

fn render_report(
    articles: &[String],
    stats: &BTreeMap<String, ArticleStats>,
    prs: &[PullRequest],
) -> String {
    let now = chrono::Utc::now().to_rfc3339();
    let zero_count = articles
        .iter()
        .filter(|a| stats.get(*a).map(|s| s.hit_count).unwrap_or(0) == 0)
        .count();

    let mut out = String::new();
    out.push_str("# Constitution Article Citation Audit\n\n");
    out.push_str(&format!("Generated: `{now}`  \n"));
    out.push_str(&format!("PRs scanned: **{}**  \n", prs.len()));
    out.push_str(&format!("Articles tracked: **{}**  \n", articles.len()));
    out.push_str(&format!(
        "Articles with zero citations: **{zero_count}**\n\n"
    ));
    out.push_str("Auto-generated by `cargo xtask audit-citations`. Sourced from `docs/governance/CONSTITUTION.md` (canonical list) + `gh pr list --state all`. Closes MVP gate criterion #5.\n\n");

    out.push_str("## Per-article citation count\n\n");
    out.push_str("| Article | Citations | Latest PR | Status |\n");
    out.push_str("|---|---|---|---|\n");
    for a in articles {
        let s = stats.get(a).cloned().unwrap_or_default();
        let latest = match (s.latest_pr, s.latest_pr_title.as_deref()) {
            (Some(n), Some(t)) => format!("#{n} — {t}"),
            (Some(n), None) => format!("#{n}"),
            _ => "—".to_string(),
        };
        let state = s.latest_pr_state.as_deref().unwrap_or("—");
        let icon = if s.hit_count == 0 { "❌" } else { "✅" };
        out.push_str(&format!(
            "| Article {a} | {} {} | {latest} | {state} |\n",
            icon, s.hit_count
        ));
    }

    if zero_count > 0 {
        out.push_str("\n## Articles needing citation coverage\n\n");
        out.push_str("Per Article VIII (stage gates) and the Stage 1 MVP gate criterion #5, every article should be cited in at least one PR. Authors of upcoming PRs should reference these articles where relevant:\n\n");
        for a in articles {
            let s = stats.get(a).cloned().unwrap_or_default();
            if s.hit_count == 0 {
                out.push_str(&format!("- **Article {a}** — no citations yet\n"));
            }
        }
    }

    out
}

impl Clone for ArticleStats {
    fn clone(&self) -> Self {
        Self {
            hit_count: self.hit_count,
            latest_pr: self.latest_pr,
            latest_pr_title: self.latest_pr_title.clone(),
            latest_pr_state: self.latest_pr_state.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_articles_from_constitution_excerpt() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            "# Constitution\n## Article I — Restraint\n\nbody\n## Article II\n## Article XIII — anti-patterns\nfoo\n",
        )
        .unwrap();
        let articles = load_canonical_articles(tmp.path()).unwrap();
        assert_eq!(articles, vec!["I", "II", "XIII"]);
    }

    #[test]
    fn rejects_invalid_roman_numerals() {
        assert!(!is_valid_roman("IIII")); // 4 should be IV
        assert!(!is_valid_roman("VV")); // not a valid form
        assert!(!is_valid_roman(""));
        assert!(!is_valid_roman("XYZ"));
        assert!(is_valid_roman("I"));
        assert!(is_valid_roman("IV"));
        assert!(is_valid_roman("XIII"));
        assert!(is_valid_roman("XXXIX")); // 39
    }

    #[test]
    fn body_citation_matches_with_word_boundary() {
        // Should match.
        assert!(body_cites_article("- Article I (restraint)", "I"));
        assert!(body_cites_article("Article XIII rule 5", "XIII"));
        assert!(body_cites_article("Article XIII rule 5", "XIII"));
        // Should NOT match — XIII shouldn't false-positive for I, V, X.
        assert!(!body_cites_article("Article XIII rule 5", "I"));
        assert!(!body_cites_article("Article XIII", "X"));
        assert!(!body_cites_article("Article XII", "I"));
        // Edge: end of string.
        assert!(body_cites_article("end with Article V", "V"));
        // The bug fix: longer-roman occurrences must not mask a later
        // real citation. A body that says "Article XIII" THEN
        // "**Article I**" earlier-find lands inside "XIII" and fails the
        // word-boundary check — but the loop continues and finds "I".
        assert!(body_cites_article(
            "Pattern: Article III — gate. Constitution: **Article I** — restraint.",
            "I"
        ));
    }

    #[test]
    fn render_report_shows_zero_citations_with_warning() {
        let articles = vec!["I".to_string(), "II".to_string(), "III".to_string()];
        let mut stats = BTreeMap::new();
        stats.insert(
            "I".to_string(),
            ArticleStats {
                hit_count: 3,
                latest_pr: Some(7),
                latest_pr_title: Some("feat: thing".to_string()),
                latest_pr_state: Some("MERGED".to_string()),
            },
        );
        // II + III have zero citations.
        let report = render_report(&articles, &stats, &[]);
        assert!(report.contains("Article I"));
        assert!(report.contains("**2**")); // zero-citation count
        assert!(report.contains("Article II"));
        assert!(report.contains("Article III"));
        assert!(report.contains("no citations yet"));
    }

    #[test]
    fn report_passes_when_all_articles_cited() {
        let articles = vec!["I".to_string()];
        let mut stats = BTreeMap::new();
        stats.insert(
            "I".to_string(),
            ArticleStats {
                hit_count: 1,
                latest_pr: Some(1),
                latest_pr_title: Some("init".to_string()),
                latest_pr_state: Some("MERGED".to_string()),
            },
        );
        let report = render_report(&articles, &stats, &[]);
        assert!(report.contains("**0**")); // zero-citation count
        assert!(!report.contains("no citations yet"));
    }
}
