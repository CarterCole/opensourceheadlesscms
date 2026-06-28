use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FieldPermission {
    #[serde(default = "default_true")]
    pub read: bool,
    #[serde(default = "default_true")]
    pub write: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContentPermissions {
    #[serde(default)]
    pub create: bool,
    #[serde(default = "default_true")]
    pub read: bool,
    #[serde(default)]
    pub update: bool,
    #[serde(default)]
    pub delete: bool,
    #[serde(default)]
    pub fields: HashMap<String, FieldPermission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub name: String,
    /// Keyed by content type name; "*" means all types
    #[serde(default)]
    pub permissions: HashMap<String, ContentPermissions>,
}

impl RoleConfig {
    pub fn permissions_for(&self, content_type: &str) -> Option<&ContentPermissions> {
        self.permissions.get(content_type)
            .or_else(|| self.permissions.get("*"))
    }
}
