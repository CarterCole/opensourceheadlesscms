use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::core::error::Result;
use crate::core::types::{
    ContentItem, ContentVersion, FieldValue, MediaItem, Pagination, SearchResult, User,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub created_by: Option<String>,
    pub tag: Option<String>,
    pub pagination: Option<Pagination>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentData {
    pub fields: HashMap<String, FieldValue>,
    pub status: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait ContentStore: Send + Sync {
    async fn get(&self, content_type: &str, id: &str) -> Result<ContentItem>;
    async fn get_by_slug(&self, content_type: &str, slug: &str) -> Result<ContentItem>;
    async fn list(&self, content_type: &str, query: ListQuery) -> Result<SearchResult>;
    async fn create(
        &self,
        content_type: &str,
        data: ContentData,
        created_by: &str,
    ) -> Result<ContentItem>;
    async fn update(
        &self,
        content_type: &str,
        id: &str,
        data: ContentData,
        updated_by: &str,
    ) -> Result<ContentItem>;
    async fn delete(&self, content_type: &str, id: &str) -> Result<()>;
    async fn transition(
        &self,
        content_type: &str,
        id: &str,
        to_state: &str,
        by_user: &str,
    ) -> Result<ContentItem>;

    // Versioning
    async fn list_versions(&self, content_type: &str, id: &str) -> Result<Vec<ContentVersion>>;
    async fn restore_version(
        &self,
        content_type: &str,
        id: &str,
        version: u32,
        by_user: &str,
    ) -> Result<ContentItem>;

    // Scheduled publishing
    async fn list_scheduled_due(&self, as_of: DateTime<Utc>) -> Result<Vec<ContentItem>>;

    // Search
    async fn search(
        &self,
        content_type: Option<&str>,
        query: &str,
        pagination: Pagination,
    ) -> Result<SearchResult>;

    // Users
    async fn get_user_by_email(&self, email: &str) -> Result<User>;
    async fn get_password_hash(&self, email: &str) -> Result<String>;
    async fn get_user(&self, id: &str) -> Result<User>;
    async fn create_user(
        &self,
        email: &str,
        display_name: &str,
        password_hash: &str,
        roles: Vec<String>,
    ) -> Result<User>;
    async fn update_user_roles(&self, id: &str, roles: Vec<String>) -> Result<User>;
    async fn list_users(&self, pagination: Pagination) -> Result<Vec<User>>;
    async fn delete_user(&self, id: &str) -> Result<()>;
    /// Current roles and session epoch for a user. A session token is only valid while
    /// its epoch matches; roles are always read from here, never trusted from the token.
    async fn get_auth_state(&self, id: &str) -> Result<(Vec<String>, i64)>;
    /// Invalidate every outstanding session token for the user (logout / revoke).
    async fn bump_session_epoch(&self, id: &str) -> Result<()>;

    // API tokens
    async fn create_api_token(
        &self,
        name: &str,
        user_id: &str,
        token_hash: &str,
        scopes: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<String>;
    async fn get_user_by_token_hash(&self, token_hash: &str) -> Result<(User, Vec<String>)>;
    async fn revoke_api_token(&self, id: &str) -> Result<()>;
    async fn touch_api_token(&self, token_hash: &str) -> Result<()>;

    // Media
    async fn create_media(
        &self,
        filename: &str,
        content_type: &str,
        size: i64,
        url: &str,
        uploaded_by: &str,
    ) -> Result<MediaItem>;
    async fn get_media(&self, id: &str) -> Result<MediaItem>;
    async fn list_media(&self, pagination: Pagination) -> Result<Vec<MediaItem>>;
    async fn delete_media(&self, id: &str) -> Result<()>;

    // Migrations
    async fn run_migrations(&self) -> Result<()>;
}
