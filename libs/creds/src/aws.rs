// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `AwsBroker` — STS AssumeRole credential fetcher.
//!
//! Pattern: terrashift_plan.md §8 + Terrashift_Plan.docx §8.
//! Constitution: Article V (short-lived STS tokens only; never holds
//! long-term keys), Article XIII rule 7 (no disk-bound secret writes).
//!
//! ## Stage 1 status
//!
//! `fetch_aws` returns `CredsError::NotImplementedYet { which:
//! "aws_sts_assume_role", session: "S5" }`. The trait surface is
//! complete; the implementation is one method swap once `aws-sdk-sts`
//! and STS test creds land in S5 close.

use crate::broker::{Credential, CredentialBroker};
use crate::errors::CredsError;
use async_trait::async_trait;

pub struct AwsBroker;

impl AwsBroker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AwsBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialBroker for AwsBroker {
    async fn resolve(&self, _name: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "aws_resolve_via_sts",
            session: "S5",
        })
    }

    async fn fetch_aws(&self, _role: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "aws_sts_assume_role",
            session: "S5",
        })
    }

    async fn fetch_gcp(&self, _account: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "aws_broker_called_for_gcp",
            session: "S5",
        })
    }

    async fn fetch_azure(&self, _subscription: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "aws_broker_called_for_azure",
            session: "S5",
        })
    }
}
