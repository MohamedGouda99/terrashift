// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;
use terrashift_cost_optimizer::{CostOptimizerError, InfracostClient, ResourceCostQuery};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn query() -> ResourceCostQuery {
    ResourceCostQuery {
        provider: "aws".to_string(),
        resource_type: "aws_instance".to_string(),
        region: "us-east-1".to_string(),
        attrs: json!({"instance_type": "t3.micro"}),
    }
}

#[tokio::test]
async fn happy_path_returns_usd() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "cost": { "usdPerMonth": 7.59, "sourceLink": "https://aws/pricing" } }
        })))
        .mount(&server)
        .await;
    let client = InfracostClient::new("test-key".to_string()).with_base_url(server.uri());
    let r = client.lookup(&query()).await.unwrap();
    assert!((r.usd_per_month - 7.59).abs() < 0.01);
    assert_eq!(r.source_link.as_deref(), Some("https://aws/pricing"));
}

#[tokio::test]
async fn rate_limit_exhausts_after_max_attempts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let client = InfracostClient::new("test-key".to_string()).with_base_url(server.uri());
    let err = client.lookup(&query()).await.unwrap_err();
    matches!(err, CostOptimizerError::RateLimitExhausted { .. });
}

#[tokio::test]
async fn missing_usd_per_month_surfaces_as_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "cost": {} }
        })))
        .mount(&server)
        .await;
    let client = InfracostClient::new("test-key".to_string()).with_base_url(server.uri());
    let err = client.lookup(&query()).await.unwrap_err();
    matches!(err, CostOptimizerError::Api(_));
}

#[tokio::test]
async fn http_500_surfaces_as_api_error_no_retry() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let client = InfracostClient::new("test-key".to_string()).with_base_url(server.uri());
    let err = client.lookup(&query()).await.unwrap_err();
    matches!(err, CostOptimizerError::Api(_));
}
