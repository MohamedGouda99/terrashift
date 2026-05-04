// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `GcpBroker` — Application Default Credentials + workload identity federation.
//!
//! Stage 1 status: trait surface complete; real ADC + WIF wires in S5
//! close.
//!
//! Constitution: Article V, Article XIII rule 7.

use crate::broker::{Credential, CredentialBroker};
use crate::errors::CredsError;
use async_trait::async_trait;

pub struct GcpBroker;

impl GcpBroker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GcpBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialBroker for GcpBroker {
    async fn resolve(&self, _name: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "gcp_resolve_via_adc",
            session: "S5",
        })
    }

    async fn fetch_aws(&self, _role: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "gcp_broker_called_for_aws",
            session: "S5",
        })
    }

    async fn fetch_gcp(&self, _account: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "gcp_adc_workload_identity_federation",
            session: "S5",
        })
    }

    async fn fetch_azure(&self, _subscription: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "gcp_broker_called_for_azure",
            session: "S5",
        })
    }
}
