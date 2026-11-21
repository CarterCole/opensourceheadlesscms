use crate::core::{
    CmsError, ContentData, ContentItem, ContentStore, ContentVersion, ListQuery, MediaItem,
    Pagination, SearchResult, User,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::info;

use crate::db::util::*;

pub struct PostgresStore {
    pool: PgPool,
    migrations_dir: String,
}

impl PostgresStore {
    pub async fn new(url: &str, migrations_dir: &str) -> Result<Self, CmsError> {
        let pool = PgPool::connect(url)
            .await
            .map_err(|e| CmsError::Storage(format!("connect: {e}")))?;
        Ok(Self {
            pool,
            migrations_dir: migrations_dir.to_string(),
        })
    }

    async fn snapshot_version(&self, item: &ContentItem) -> Result<(), CmsError> {
        let id = new_id();
        let fields_json = serde_json::to_string(&item.fields)?;
        let created_at = now_str();

        sqlx::query(
            "INSERT INTO _cms_content_versions (id, content_id, content_type, version, fields, status, created_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(&id)
        .bind(&item.id)
        .bind(&item.content_type)
        .bind(item.version as i64)
        .bind(&fields_json)
        .bind(&item.status)
        .bind(&item.created_by)
        .bind(&created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("snapshot version: {e}")))?;

        Ok(())
    }

    async fn update_search_index(&self, item: &ContentItem) -> Result<(), CmsError> {
        let text_blob: String = item
            .fields
            .values()
            .filter_map(|v| match v {
                crate::core::FieldValue::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");

        sqlx::query("DELETE FROM _cms_search_index WHERE id = $1")
            .bind(&item.id)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("search index delete: {e}")))?;

        sqlx::query(
            "INSERT INTO _cms_search_index (id, content_type, text_blob) VALUES ($1, $2, $3)",
        )
        .bind(&item.id)
        .bind(&item.content_type)
        .bind(&text_blob)
        .execute(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("search index insert: {e}")))?;

        Ok(())
    }
}

type ContentRow = (
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
    String,
    Option<String>,
);

impl PostgresStore {
    /// Apply any `NNN_name.sql` files in the migrations directory that haven't run yet.
    /// sqlx records them in `_sqlx_migrations` and runs each in its own transaction.
    async fn apply_migrations(&self) -> Result<(), CmsError> {
        let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(&self.migrations_dir))
            .await
            .map_err(|e| CmsError::Storage(format!("load migrations: {e}")))?;
        migrator
            .run(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("run migrations: {e}")))?;
        info!("migrations applied");
        Ok(())
    }
}

#[async_trait]
impl ContentStore for PostgresStore {
    async fn get(&self, content_type: &str, id: &str) -> Result<ContentItem, CmsError> {
        let row = sqlx::query_as::<_, ContentRow>(
            "SELECT id, content_type, status, fields, version, created_at, updated_at, created_by, updated_by, scheduled_at
             FROM _cms_content WHERE id = $1 AND content_type = $2"
        )
        .bind(id)
        .bind(content_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("get: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("{content_type}/{id}")))?;

        row_to_content_item(
            &row.0,
            &row.1,
            &row.2,
            &row.3,
            row.4,
            &row.5,
            &row.6,
            &row.7,
            &row.8,
            row.9.as_deref(),
        )
    }

    async fn get_by_slug(&self, content_type: &str, slug: &str) -> Result<ContentItem, CmsError> {
        let row = sqlx::query_as::<_, ContentRow>(
            "SELECT id, content_type, status, fields, version, created_at, updated_at, created_by, updated_by, scheduled_at
             FROM _cms_content WHERE content_type = $1 AND (fields::jsonb)->>'slug' = $2"
        )
        .bind(content_type)
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("get_by_slug: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("{content_type}/slug:{slug}")))?;

        row_to_content_item(
            &row.0,
            &row.1,
            &row.2,
            &row.3,
            row.4,
            &row.5,
            &row.6,
            &row.7,
            &row.8,
            row.9.as_deref(),
        )
    }

    async fn list(&self, content_type: &str, query: ListQuery) -> Result<SearchResult, CmsError> {
        let pagination = query.pagination.clone().unwrap_or_default();
        let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;

        let (rows, total) = if let Some(ref status) = query.status {
            let rows = sqlx::query_as::<_, ContentRow>(
                "SELECT id, content_type, status, fields, version, created_at, updated_at, created_by, updated_by, scheduled_at
                 FROM _cms_content WHERE content_type = $1 AND status = $2
                 ORDER BY updated_at DESC LIMIT $3 OFFSET $4"
            )
            .bind(content_type).bind(status)
            .bind(pagination.per_page as i64).bind(offset as i64)
            .fetch_all(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("list: {e}")))?;

            let total = sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*) FROM _cms_content WHERE content_type = $1 AND status = $2",
            )
            .bind(content_type)
            .bind(status)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("list count: {e}")))?
            .0 as u64;

            (rows, total)
        } else {
            let rows = sqlx::query_as::<_, ContentRow>(
                "SELECT id, content_type, status, fields, version, created_at, updated_at, created_by, updated_by, scheduled_at
                 FROM _cms_content WHERE content_type = $1
                 ORDER BY updated_at DESC LIMIT $2 OFFSET $3"
            )
            .bind(content_type)
            .bind(pagination.per_page as i64).bind(offset as i64)
            .fetch_all(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("list: {e}")))?;

            let total = sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*) FROM _cms_content WHERE content_type = $1",
            )
            .bind(content_type)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("list count: {e}")))?
            .0 as u64;

            (rows, total)
        };

        let items = rows
            .iter()
            .map(|r| {
                row_to_content_item(
                    &r.0,
                    &r.1,
                    &r.2,
                    &r.3,
                    r.4,
                    &r.5,
                    &r.6,
                    &r.7,
                    &r.8,
                    r.9.as_deref(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SearchResult {
            items,
            total,
            page: pagination.page,
            per_page: pagination.per_page,
        })
    }

    async fn create(
        &self,
        content_type: &str,
        data: ContentData,
        created_by: &str,
    ) -> Result<ContentItem, CmsError> {
        let id = new_id();
        let now = now_str();
        let status = data.status.as_deref().unwrap_or("draft").to_string();
        let fields_json = serde_json::to_string(&data.fields)?;
        let scheduled_at = data.scheduled_at.as_ref().map(|dt| dt.to_rfc3339());

        sqlx::query(
            "INSERT INTO _cms_content (id, content_type, status, fields, version, created_at, updated_at, created_by, updated_by, scheduled_at)
             VALUES ($1, $2, $3, $4, 1, $5, $6, $7, $8, $9)"
        )
        .bind(&id)
        .bind(content_type)
        .bind(&status)
        .bind(&fields_json)
        .bind(&now)
        .bind(&now)
        .bind(created_by)
        .bind(created_by)
        .bind(scheduled_at.as_deref())
        .execute(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("create: {e}")))?;

        let item = self.get(content_type, &id).await?;
        self.update_search_index(&item).await?;
        info!("created {content_type}/{id}");
        Ok(item)
    }

    async fn update(
        &self,
        content_type: &str,
        id: &str,
        data: ContentData,
        updated_by: &str,
    ) -> Result<ContentItem, CmsError> {
        let existing = self.get(content_type, id).await?;
        self.snapshot_version(&existing).await?;

        let now = now_str();
        let fields_json = serde_json::to_string(&data.fields)?;
        let status = data
            .status
            .as_deref()
            .unwrap_or(&existing.status)
            .to_string();
        let scheduled_at = data
            .scheduled_at
            .as_ref()
            .map(|dt| dt.to_rfc3339())
            .or_else(|| existing.scheduled_at.map(|dt| dt.to_rfc3339()));

        sqlx::query(
            "UPDATE _cms_content SET fields = $1, status = $2, updated_at = $3, updated_by = $4, scheduled_at = $5, version = version + 1
             WHERE id = $6 AND content_type = $7"
        )
        .bind(&fields_json)
        .bind(&status)
        .bind(&now)
        .bind(updated_by)
        .bind(scheduled_at.as_deref())
        .bind(id)
        .bind(content_type)
        .execute(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("update: {e}")))?;

        let item = self.get(content_type, id).await?;
        self.update_search_index(&item).await?;
        Ok(item)
    }

    async fn delete(&self, content_type: &str, id: &str) -> Result<(), CmsError> {
        sqlx::query("DELETE FROM _cms_content WHERE id = $1 AND content_type = $2")
            .bind(id)
            .bind(content_type)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("delete: {e}")))?;
        sqlx::query("DELETE FROM _cms_search_index WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("search index delete: {e}")))?;
        Ok(())
    }

    async fn transition(
        &self,
        content_type: &str,
        id: &str,
        to_state: &str,
        by_user: &str,
    ) -> Result<ContentItem, CmsError> {
        let existing = self.get(content_type, id).await?;
        self.snapshot_version(&existing).await?;

        let now = now_str();
        sqlx::query(
            "UPDATE _cms_content SET status = $1, updated_at = $2, updated_by = $3, version = version + 1
             WHERE id = $4 AND content_type = $5"
        )
        .bind(to_state).bind(&now).bind(by_user).bind(id).bind(content_type)
        .execute(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("transition: {e}")))?;

        self.get(content_type, id).await
    }

    async fn list_versions(
        &self,
        content_type: &str,
        id: &str,
    ) -> Result<Vec<ContentVersion>, CmsError> {
        let rows = sqlx::query_as::<_, (String, String, String, i64, String, String, String, String)>(
            "SELECT id, content_id, content_type, version, fields, status, created_by, created_at
             FROM _cms_content_versions WHERE content_id = $1 AND content_type = $2
             ORDER BY version DESC"
        )
        .bind(id).bind(content_type)
        .fetch_all(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("list_versions: {e}")))?;

        rows.iter()
            .map(|r| row_to_version(&r.0, &r.1, &r.2, r.3, &r.4, &r.5, &r.6, &r.7))
            .collect()
    }

    async fn restore_version(
        &self,
        content_type: &str,
        id: &str,
        version: u32,
        by_user: &str,
    ) -> Result<ContentItem, CmsError> {
        let v = sqlx::query_as::<_, (String, String)>(
            "SELECT fields, status FROM _cms_content_versions
             WHERE content_id = $1 AND content_type = $2 AND version = $3",
        )
        .bind(id)
        .bind(content_type)
        .bind(version as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("restore_version fetch: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("version {version} for {content_type}/{id}")))?;

        let fields = serde_json::from_str(&v.0)?;
        let data = ContentData {
            fields,
            status: Some(v.1),
            scheduled_at: None,
        };
        self.update(content_type, id, data, by_user).await
    }

    async fn list_scheduled_due(&self, as_of: DateTime<Utc>) -> Result<Vec<ContentItem>, CmsError> {
        let as_of_str = as_of.to_rfc3339();
        let rows = sqlx::query_as::<_, ContentRow>(
            "SELECT id, content_type, status, fields, version, created_at, updated_at, created_by, updated_by, scheduled_at
             FROM _cms_content WHERE status = 'approved' AND scheduled_at IS NOT NULL AND scheduled_at <= $1"
        )
        .bind(&as_of_str)
        .fetch_all(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("list_scheduled_due: {e}")))?;

        rows.iter()
            .map(|r| {
                row_to_content_item(
                    &r.0,
                    &r.1,
                    &r.2,
                    &r.3,
                    r.4,
                    &r.5,
                    &r.6,
                    &r.7,
                    &r.8,
                    r.9.as_deref(),
                )
            })
            .collect()
    }

    async fn search(
        &self,
        content_type: Option<&str>,
        query: &str,
        pagination: Pagination,
    ) -> Result<SearchResult, CmsError> {
        let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;

        let (rows, total) = if let Some(ct) = content_type {
            let rows = sqlx::query_as::<_, ContentRow>(
                "SELECT c.id, c.content_type, c.status, c.fields, c.version, c.created_at, c.updated_at, c.created_by, c.updated_by, c.scheduled_at
                 FROM _cms_search_index si JOIN _cms_content c ON si.id = c.id
                 WHERE si.content_type = $1 AND to_tsvector('english', si.text_blob) @@ plainto_tsquery('english', $2)
                 ORDER BY c.updated_at DESC LIMIT $3 OFFSET $4"
            )
            .bind(ct).bind(query)
            .bind(pagination.per_page as i64).bind(offset as i64)
            .fetch_all(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("search: {e}")))?;

            let total = sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*) FROM _cms_search_index
                 WHERE content_type = $1 AND to_tsvector('english', text_blob) @@ plainto_tsquery('english', $2)"
            )
            .bind(ct).bind(query)
            .fetch_one(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("search count: {e}")))?.0 as u64;

            (rows, total)
        } else {
            let rows = sqlx::query_as::<_, ContentRow>(
                "SELECT c.id, c.content_type, c.status, c.fields, c.version, c.created_at, c.updated_at, c.created_by, c.updated_by, c.scheduled_at
                 FROM _cms_search_index si JOIN _cms_content c ON si.id = c.id
                 WHERE to_tsvector('english', si.text_blob) @@ plainto_tsquery('english', $1)
                 ORDER BY c.updated_at DESC LIMIT $2 OFFSET $3"
            )
            .bind(query)
            .bind(pagination.per_page as i64).bind(offset as i64)
            .fetch_all(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("search: {e}")))?;

            let total = sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*) FROM _cms_search_index
                 WHERE to_tsvector('english', text_blob) @@ plainto_tsquery('english', $1)",
            )
            .bind(query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("search count: {e}")))?
            .0 as u64;

            (rows, total)
        };

        let items = rows
            .iter()
            .map(|r| {
                row_to_content_item(
                    &r.0,
                    &r.1,
                    &r.2,
                    &r.3,
                    r.4,
                    &r.5,
                    &r.6,
                    &r.7,
                    &r.8,
                    r.9.as_deref(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SearchResult {
            items,
            total,
            page: pagination.page,
            per_page: pagination.per_page,
        })
    }

    // --- Users ---

    async fn get_user_by_email(&self, email: &str) -> Result<User, CmsError> {
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, String, String)>(
            "SELECT id, email, display_name, bio, avatar_url, roles, created_at FROM _cms_users WHERE email = $1"
        )
        .bind(email)
        .fetch_optional(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("get_user_by_email: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("user:{email}")))?;

        row_to_user(
            &row.0,
            &row.1,
            &row.2,
            row.3.as_deref(),
            row.4.as_deref(),
            &row.5,
            &row.6,
        )
    }

    async fn get_user_by_email_ci(&self, email: &str) -> Result<User, CmsError> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, String, String)>(
            "SELECT id, email, display_name, bio, avatar_url, roles, created_at FROM _cms_users WHERE lower(email) = lower($1) LIMIT 2"
        )
        .bind(email)
        .fetch_all(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("get_user_by_email_ci: {e}")))?;

        match rows.as_slice() {
            [] => Err(CmsError::NotFound(format!("user:{email}"))),
            [row] => row_to_user(
                &row.0,
                &row.1,
                &row.2,
                row.3.as_deref(),
                row.4.as_deref(),
                &row.5,
                &row.6,
            ),
            _ => Err(CmsError::Validation(format!(
                "several accounts match {email} ignoring case"
            ))),
        }
    }

    async fn get_password_hash(&self, email: &str) -> Result<String, CmsError> {
        let row =
            sqlx::query_as::<_, (String,)>("SELECT password_hash FROM _cms_users WHERE email = $1")
                .bind(email)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| CmsError::Storage(format!("get_password_hash: {e}")))?
                .ok_or_else(|| CmsError::NotFound(format!("user:{email}")))?;
        Ok(row.0)
    }

    async fn get_user(&self, id: &str) -> Result<User, CmsError> {
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, String, String)>(
            "SELECT id, email, display_name, bio, avatar_url, roles, created_at FROM _cms_users WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("get_user: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("user:{id}")))?;

        row_to_user(
            &row.0,
            &row.1,
            &row.2,
            row.3.as_deref(),
            row.4.as_deref(),
            &row.5,
            &row.6,
        )
    }

    async fn create_user(
        &self,
        email: &str,
        display_name: &str,
        password_hash: &str,
        roles: Vec<String>,
    ) -> Result<User, CmsError> {
        let id = new_id();
        let now = now_str();
        let roles_json = serde_json::to_string(&roles)?;

        sqlx::query(
            "INSERT INTO _cms_users (id, email, display_name, password_hash, roles, created_at) VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(&id).bind(email).bind(display_name).bind(password_hash).bind(&roles_json).bind(&now)
        .execute(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("create_user: {e}")))?;

        self.get_user(&id).await
    }

    async fn update_user_roles(&self, id: &str, roles: Vec<String>) -> Result<User, CmsError> {
        let roles_json = serde_json::to_string(&roles)?;
        sqlx::query("UPDATE _cms_users SET roles = $1 WHERE id = $2")
            .bind(&roles_json)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("update_user_roles: {e}")))?;
        self.get_user(id).await
    }

    async fn list_users(&self, pagination: Pagination) -> Result<Vec<User>, CmsError> {
        let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, String, String)>(
            "SELECT id, email, display_name, bio, avatar_url, roles, created_at FROM _cms_users ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(pagination.per_page as i64).bind(offset as i64)
        .fetch_all(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("list_users: {e}")))?;

        rows.iter()
            .map(|r| row_to_user(&r.0, &r.1, &r.2, r.3.as_deref(), r.4.as_deref(), &r.5, &r.6))
            .collect()
    }

    async fn delete_user(&self, id: &str) -> Result<(), CmsError> {
        sqlx::query("DELETE FROM _cms_users WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("delete_user: {e}")))?;
        Ok(())
    }

    // --- API Tokens ---

    async fn create_api_token(
        &self,
        name: &str,
        user_id: &str,
        token_hash: &str,
        scopes: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<String, CmsError> {
        let id = new_id();
        let now = now_str();
        let scopes_json = serde_json::to_string(&scopes)?;
        let exp = expires_at.map(|dt| dt.to_rfc3339());

        sqlx::query(
            "INSERT INTO _cms_api_tokens (id, name, token_hash, user_id, scopes, expires_at, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(&id).bind(name).bind(token_hash).bind(user_id).bind(&scopes_json).bind(exp.as_deref()).bind(&now)
        .execute(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("create_api_token: {e}")))?;

        Ok(id)
    }

    async fn get_user_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<(User, Vec<String>), CmsError> {
        let now = now_str();
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT user_id, scopes FROM _cms_api_tokens
             WHERE token_hash = $1 AND (expires_at IS NULL OR expires_at > $2)",
        )
        .bind(token_hash)
        .bind(&now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("get_user_by_token_hash: {e}")))?
        .ok_or(CmsError::Unauthorized)?;

        let scopes: Vec<String> = serde_json::from_str(&row.1)?;
        let user = self.get_user(&row.0).await?;
        Ok((user, scopes))
    }

    async fn revoke_api_token(&self, id: &str) -> Result<(), CmsError> {
        sqlx::query("DELETE FROM _cms_api_tokens WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("revoke_api_token: {e}")))?;
        Ok(())
    }

    async fn touch_api_token(&self, token_hash: &str) -> Result<(), CmsError> {
        let now = now_str();
        sqlx::query("UPDATE _cms_api_tokens SET last_used = $1 WHERE token_hash = $2")
            .bind(&now)
            .bind(token_hash)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("touch_api_token: {e}")))?;
        Ok(())
    }

    // --- Media ---

    async fn create_media(
        &self,
        filename: &str,
        content_type: &str,
        size: i64,
        url: &str,
        uploaded_by: &str,
    ) -> Result<MediaItem, CmsError> {
        let id = new_id();
        let now = now_str();

        sqlx::query(
            "INSERT INTO _cms_media (id, filename, content_type, size, url, uploaded_by, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(&id).bind(filename).bind(content_type).bind(size).bind(url).bind(uploaded_by).bind(&now)
        .execute(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("create_media: {e}")))?;

        self.get_media(&id).await
    }

    async fn get_media(&self, id: &str) -> Result<MediaItem, CmsError> {
        let row = sqlx::query_as::<_, (String, String, String, i64, String, String, String)>(
            "SELECT id, filename, content_type, size, url, uploaded_by, created_at FROM _cms_media WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("get_media: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("media:{id}")))?;

        row_to_media(&row.0, &row.1, &row.2, row.3, &row.4, &row.5, &row.6)
    }

    async fn list_media(&self, pagination: Pagination) -> Result<Vec<MediaItem>, CmsError> {
        let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;
        let rows = sqlx::query_as::<_, (String, String, String, i64, String, String, String)>(
            "SELECT id, filename, content_type, size, url, uploaded_by, created_at FROM _cms_media ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(pagination.per_page as i64).bind(offset as i64)
        .fetch_all(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("list_media: {e}")))?;

        rows.iter()
            .map(|r| row_to_media(&r.0, &r.1, &r.2, r.3, &r.4, &r.5, &r.6))
            .collect()
    }

    async fn delete_media(&self, id: &str) -> Result<(), CmsError> {
        sqlx::query("DELETE FROM _cms_media WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("delete_media: {e}")))?;
        Ok(())
    }

    // --- Migrations ---

    async fn run_migrations(&self) -> Result<(), CmsError> {
        self.apply_migrations().await
    }

    async fn get_auth_state(&self, id: &str) -> Result<(Vec<String>, i64), CmsError> {
        let row = sqlx::query_as::<_, (String, i64)>(
            "SELECT roles, session_epoch FROM _cms_users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("get_auth_state: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("user:{id}")))?;
        let roles: Vec<String> = serde_json::from_str(&row.0)
            .map_err(|e| CmsError::Storage(format!("roles deserialize: {e}")))?;
        Ok((roles, row.1))
    }

    async fn bump_session_epoch(&self, id: &str) -> Result<(), CmsError> {
        sqlx::query("UPDATE _cms_users SET session_epoch = session_epoch + 1 WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("bump_session_epoch: {e}")))?;
        Ok(())
    }
}
