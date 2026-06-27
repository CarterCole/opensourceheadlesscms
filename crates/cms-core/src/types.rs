use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContentItem {
    pub id: String,
    pub content_type: String,
    pub status: String,
    pub fields: HashMap<String, FieldValue>,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: String,
    pub scheduled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FieldValue {
    Text(String),
    Number(f64),
    Bool(bool),
    Date(DateTime<Utc>),
    Blocks(Vec<Block>),
    Relation(Vec<String>),
    List(Vec<FieldValue>),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Block {
    pub block_type: BlockType,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Paragraph,
    Heading,
    Image,
    Code,
    Embed,
    Quote,
    Divider,
    Component,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentVersion {
    pub id: String,
    pub content_id: String,
    pub content_type: String,
    pub version: u32,
    pub fields: HashMap<String, FieldValue>,
    pub status: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub roles: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub url: String,
    pub uploaded_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub user_id: String,
    pub action: String,
    pub content_type: Option<String>,
    pub content_id: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u32,
    pub per_page: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self { page: 1, per_page: 20 }
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub items: Vec<ContentItem>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
}
