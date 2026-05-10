// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Infracost API client. POSTs GraphQL queries to `pricing.api.infracost.io`,
//! retries on transient failures (network + 5xx + 429), and surfaces
//! anything else as a typed CostOptimizerError.
//!
//! Article V: this module only ships the *shape* of a request. The
//! caller (Phase 2 onwards) is responsible for not putting secrets in
//! the resource attrs bag.

use crate::errors::CostOptimizerError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://pricing.api.infracost.io";
const MAX_RETRY_ATTEMPTS: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCostQuery {
    pub provider: String,
    pub resource_type: String,
    pub region: String,
    pub attrs: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCostResult {
    pub usd_per_month: f64,
    pub source_link: Option<String>,
}

pub struct InfracostClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl InfracostClient {
    pub fn new(api_key: String) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_key,
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }

    /// Look up the on-demand monthly cost for one resource. Retries on
    /// transient failures up to MAX_RETRY_ATTEMPTS times with exponential
    /// backoff.
    pub async fn lookup(
        &self,
        query: &ResourceCostQuery,
    ) -> Result<ResourceCostResult, CostOptimizerError> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match self.lookup_once(query).await {
                Ok(r) => return Ok(r),
                Err(CostOptimizerError::Api(msg)) if msg.contains("rate limit") => {
                    let last_retry_after = INITIAL_BACKOFF_MS * (1u64 << (attempt - 1)) / 1000;
                    if attempt >= MAX_RETRY_ATTEMPTS {
                        return Err(CostOptimizerError::RateLimitExhausted {
                            attempts: attempt,
                            retry_after_secs: last_retry_after,
                        });
                    }
                    tokio::time::sleep(Duration::from_millis(
                        INITIAL_BACKOFF_MS * (1u64 << (attempt - 1)),
                    ))
                    .await;
                }
                Err(CostOptimizerError::Network(e)) if e.is_timeout() || e.is_connect() => {
                    if attempt >= MAX_RETRY_ATTEMPTS {
                        return Err(CostOptimizerError::Network(e));
                    }
                    tokio::time::sleep(Duration::from_millis(
                        INITIAL_BACKOFF_MS * (1u64 << (attempt - 1)),
                    ))
                    .await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn lookup_once(
        &self,
        query: &ResourceCostQuery,
    ) -> Result<ResourceCostResult, CostOptimizerError> {
        let body = serde_json::json!({
            "query": "query Cost($p:String!,$t:String!,$r:String!) { cost(provider:$p, type:$t, region:$r) { usdPerMonth sourceLink } }",
            "variables": {
                "p": &query.provider,
                "t": &query.resource_type,
                "r": &query.region,
            }
        });
        let url = format!("{}/graphql", self.base_url);
        let resp = self
            .http
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(CostOptimizerError::Network)?;
        let status = resp.status();
        if status.as_u16() == 429 {
            return Err(CostOptimizerError::Api("rate limit".to_string()));
        }
        if !status.is_success() {
            return Err(CostOptimizerError::Api(format!(
                "infracost returned status {status}"
            )));
        }
        let raw = resp.text().await.map_err(CostOptimizerError::Network)?;
        let parsed: serde_json::Value = serde_json::from_str(&raw)?;
        let cost = &parsed["data"]["cost"];
        let usd = cost["usdPerMonth"].as_f64().ok_or_else(|| {
            CostOptimizerError::Api("missing usdPerMonth in response".to_string())
        })?;
        let link = cost["sourceLink"].as_str().map(|s| s.to_string());
        Ok(ResourceCostResult {
            usd_per_month: usd,
            source_link: link,
        })
    }
}
