//! `AzureBroker` — managed identity / service principal short-lived tokens.
//!
//! Stage 1 status: trait surface complete; real `azure-identity`
//! integration wires in S5 close.
//!
//! Constitution: Article V, Article XIII rule 7.

use crate::broker::{Credential, CredentialBroker};
use crate::errors::CredsError;
use async_trait::async_trait;

pub struct AzureBroker;

impl AzureBroker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AzureBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialBroker for AzureBroker {
    async fn resolve(&self, _name: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "azure_resolve_via_managed_identity",
            session: "S5",
        })
    }

    async fn fetch_aws(&self, _role: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "azure_broker_called_for_aws",
            session: "S5",
        })
    }

    async fn fetch_gcp(&self, _account: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "azure_broker_called_for_gcp",
            session: "S5",
        })
    }

    async fn fetch_azure(&self, _subscription: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "azure_managed_identity",
            session: "S5",
        })
    }
}
