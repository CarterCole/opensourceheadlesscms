use crate::core::error::Result;
use crate::core::scripting::ComponentRegistry;
use crate::core::types::{AuditEntry, ContentItem};
use std::collections::HashMap;

pub struct Credentials {
    pub provider: String,
    pub payload: HashMap<String, String>,
}

pub struct AuthenticatedUser {
    pub user_id: String,
    pub roles: Vec<String>,
}

pub trait AuthProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn authenticate(&self, credentials: &Credentials) -> Result<AuthenticatedUser>;
}

pub trait ExtensionPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;

    fn auth_provider(&self) -> Option<Box<dyn AuthProvider>> {
        None
    }

    fn on_content_create(&self, _item: &ContentItem) -> Result<()> {
        Ok(())
    }
    fn on_content_update(&self, _old: &ContentItem, _new: &ContentItem) -> Result<()> {
        Ok(())
    }
    fn on_content_delete(&self, _item: &ContentItem) -> Result<()> {
        Ok(())
    }
    fn on_workflow_transition(&self, _item: &ContentItem, _to: &str) -> Result<()> {
        Ok(())
    }

    fn audit_entry(&self, _entry: &AuditEntry) -> Result<()> {
        Ok(())
    }

    fn register_components(&self, _registry: &mut dyn ComponentRegistry) {}
}
