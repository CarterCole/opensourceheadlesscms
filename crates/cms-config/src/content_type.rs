use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    Database,
    Flatfile,
}

impl Default for StorageBackend {
    fn default() -> Self { Self::Database }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    RichText,
    Slug,
    Number,
    Bool,
    Date,
    Image,
    Relation {
        target: String,
        #[serde(default = "default_cardinality")]
        cardinality: String,
    },
    #[serde(rename = "list")]
    List {
        of: Box<FieldType>,
    },
}

fn default_cardinality() -> String { "one".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FieldAccess {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub list: bool,
    pub source: Option<String>,
    pub access: Option<FieldAccess>,
    pub default: Option<serde_json::Value>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentTypeConfig {
    pub name: String,
    pub label: Option<String>,
    #[serde(default)]
    pub storage: StorageBackend,
    #[serde(default)]
    pub fields: Vec<FieldConfig>,
    pub workflow: Option<String>,
    #[serde(default)]
    pub searchable: bool,
}

impl ContentTypeConfig {
    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }

    pub fn field(&self, name: &str) -> Option<&FieldConfig> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn restricted_fields(&self) -> HashMap<String, &FieldAccess> {
        self.fields.iter()
            .filter_map(|f| f.access.as_ref().map(|a| (f.name.clone(), a)))
            .collect()
    }
}
