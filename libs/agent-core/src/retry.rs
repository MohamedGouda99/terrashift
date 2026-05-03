//! Retry policy — exponential backoff with header-driven override.
//!
//! Pattern: the architecture reference §8 (kernel retry primitive).
//! Source: the reference codebase (see ATTRIBUTIONS.md) (verbatim port — the
//! the reference impl is a clean ~185-line stateless function set; no Stage 2
//! narrowing needed).
//!
//! Constitution: Article IV (loud errors — exhausted retries surface as
//! `AgentError::Inference`).
//!
//! Header precedence (HTTP retry semantics):
//!   1. `retry-after-ms` (milliseconds) — Anthropic / OpenAI extension
//!   2. `retry-after` (integer seconds) — RFC 7231
//!   3. `retry-after` (HTTP date) — RFC 7231 fallback
//!
//! When no header is present, the kernel falls back to exponential
//! backoff: `delay = initial_backoff_ms * multiplier^(attempt-1)`,
//! clamped to `[initial_backoff_ms, max_backoff_ms]`.

use crate::types::RetryConfig;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Retry metadata parsed from provider responses or computed locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryDelay {
    pub delay_ms: u64,
    pub source: RetryDelaySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDelaySource {
    RetryAfterMsHeader,
    RetryAfterHeader,
    ExponentialBackoff,
}

/// Parse a retry delay from response headers.
pub fn parse_retry_delay_from_headers(
    headers: &HashMap<String, String>,
    now: DateTime<Utc>,
) -> Option<RetryDelay> {
    if let Some(raw_ms) = find_header(headers, "retry-after-ms") {
        if let Ok(delay_ms) = raw_ms.trim().parse::<u64>() {
            return Some(RetryDelay {
                delay_ms,
                source: RetryDelaySource::RetryAfterMsHeader,
            });
        }
    }

    let raw_retry_after = find_header(headers, "retry-after")?;
    let raw_retry_after = raw_retry_after.trim();

    if let Ok(seconds) = raw_retry_after.parse::<u64>() {
        return Some(RetryDelay {
            delay_ms: seconds.saturating_mul(1_000),
            source: RetryDelaySource::RetryAfterHeader,
        });
    }

    let date = DateTime::parse_from_rfc2822(raw_retry_after).ok()?;
    let date = date.with_timezone(&Utc);
    let diff_ms = (date - now).num_milliseconds();
    if diff_ms <= 0 {
        return Some(RetryDelay {
            delay_ms: 0,
            source: RetryDelaySource::RetryAfterHeader,
        });
    }

    Some(RetryDelay {
        delay_ms: u64::try_from(diff_ms).unwrap_or(0),
        source: RetryDelaySource::RetryAfterHeader,
    })
}

/// Compute exponential backoff delay for `attempt` (1-indexed).
pub fn exponential_backoff_ms(config: &RetryConfig, attempt: usize) -> u64 {
    if attempt <= 1 {
        return config.initial_backoff_ms.min(config.max_backoff_ms);
    }

    // attempt-1 fits in i32 for any practical attempt count; saturate.
    let exponent = i32::try_from(attempt.saturating_sub(1)).unwrap_or(i32::MAX);
    let factor = config.multiplier.powi(exponent);
    #[allow(clippy::cast_precision_loss)]
    let delay = (config.initial_backoff_ms as f64) * factor;

    if delay.is_nan() || delay.is_sign_negative() {
        return config.initial_backoff_ms.min(config.max_backoff_ms);
    }

    #[allow(clippy::cast_precision_loss)]
    let cap = config.max_backoff_ms as f64;
    let clamped = delay.min(cap);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        clamped as u64
    }
}

/// Resolve the final retry delay: prefer header-driven, fall back to
/// exponential backoff. The returned `RetryDelay::source` records which
/// path was taken (useful for audit-log entries).
pub fn resolve_retry_delay_ms(
    headers: &HashMap<String, String>,
    config: &RetryConfig,
    attempt: usize,
    now: DateTime<Utc>,
) -> RetryDelay {
    if let Some(parsed) = parse_retry_delay_from_headers(headers, now) {
        return parsed;
    }

    RetryDelay {
        delay_ms: exponential_backoff_ms(config, attempt),
        source: RetryDelaySource::ExponentialBackoff,
    }
}

fn find_header<'a>(headers: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_key, _)| header_key.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn base_config() -> RetryConfig {
        RetryConfig {
            max_attempts: 4,
            initial_backoff_ms: 2_000,
            max_backoff_ms: 30_000,
            multiplier: 2.0,
        }
    }

    #[test]
    fn parse_retry_after_ms_header_takes_precedence() {
        let mut headers = HashMap::new();
        headers.insert("retry-after-ms".to_string(), "1500".to_string());
        headers.insert("retry-after".to_string(), "20".to_string());

        let parsed = parse_retry_delay_from_headers(&headers, Utc::now());
        assert_eq!(
            parsed,
            Some(RetryDelay {
                delay_ms: 1_500,
                source: RetryDelaySource::RetryAfterMsHeader,
            })
        );
    }

    #[test]
    fn parse_retry_after_seconds_header() {
        let mut headers = HashMap::new();
        headers.insert("Retry-After".to_string(), "3".to_string());

        let parsed = parse_retry_delay_from_headers(&headers, Utc::now());
        assert_eq!(
            parsed,
            Some(RetryDelay {
                delay_ms: 3_000,
                source: RetryDelaySource::RetryAfterHeader,
            })
        );
    }

    #[test]
    fn parse_retry_after_http_date_header() {
        let now = Utc::now();
        let target = now + Duration::seconds(5);
        let mut headers = HashMap::new();
        headers.insert("retry-after".to_string(), target.to_rfc2822());

        let parsed = parse_retry_delay_from_headers(&headers, now).expect("parsed retry delay");

        assert_eq!(parsed.source, RetryDelaySource::RetryAfterHeader);
        assert!((4_000..=6_000).contains(&parsed.delay_ms));
    }

    #[test]
    fn exponential_backoff_respects_cap() {
        let config = base_config();

        assert_eq!(exponential_backoff_ms(&config, 1), 2_000);
        assert_eq!(exponential_backoff_ms(&config, 2), 4_000);
        assert_eq!(exponential_backoff_ms(&config, 3), 8_000);
        assert_eq!(exponential_backoff_ms(&config, 10), 30_000);
    }

    #[test]
    fn resolve_retry_delay_falls_back_to_backoff() {
        let config = base_config();
        let headers = HashMap::new();

        let resolved = resolve_retry_delay_ms(&headers, &config, 3, Utc::now());
        assert_eq!(resolved.delay_ms, 8_000);
        assert_eq!(resolved.source, RetryDelaySource::ExponentialBackoff);
    }
}
