use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crate::core::{CmsError, ContentData, ContentItem, ContentStore, ContentVersion, ListQuery, MediaItem, Pagination, SearchResult, User};
use sqlx::SqlitePool;
use tracing::info;

use crate::db::util::*;

pub struct SqliteStore {
    pool: SqlitePool,
    migrations_dir: String,
}

impl SqliteStore {
    pub async fn new(url: &str, migrations_dir: &str) -> Result<Self, CmsError> {
        let pool = SqlitePool::connect(url).await
            .map_err(|e| CmsError::Storage(format!("connect: {e}")))?;
        Ok(Self { pool, migrations_dir: migrations_dir.to_string() })
    }

    async fn snapshot_version(&self, item: &ContentItem) -> Result<(), CmsError> {
        let id = new_id();
        let fields_json = serde_json::to_string(&item.fields)?;
        let created_at = now_str();

        sqlx::query(
            "INSERT INTO _cms_content_versions (id, content_id, content_type, version, fields, status, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
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

    async fn update_fts(&self, item: &ContentItem) -> Result<(), CmsError> {
        // Concatenate all text field values for FTS indexing
        let text_blob: String = item.fields.values()
            .filter_map(|v| match v {
                crate::core::FieldValue::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");

        // FTS5 update: delete old then insert new
        sqlx::query("DELETE FROM _cms_search_fts WHERE id = ?")
            .bind(&item.id)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("fts delete: {e}")))?;

        sqlx::query("INSERT INTO _cms_search_fts (id, content_type, text_blob) VALUES (?, ?, ?)")
            .bind(&item.id)
            .bind(&item.content_type)
            .bind(&text_blob)
            .execute(&self.pool)
            .await
            .map_err(|e| CmsError::Storage(format!("fts insert: {e}")))?;

        Ok(())
    }
}

#[async_trait]
impl ContentStore for SqliteStore {
    async fn get(&self, content_type: &str, id: &str) -> Result<ContentItem, CmsError> {
        let row = sqlx::query_as::<_, (String, String, String, String, i64, String, String, String, Option<String>)>(
            "SELECT id, content_type, status, fields, version, created_at, updated_at, created_by, scheduled_at
             FROM _cms_content WHERE id = ? AND content_type = ?"
        )
        .bind(id)
        .bind(content_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("get: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("{content_type}/{id}")))?;

        row_to_content_item(&row.0, &row.1, &row.2, &row.3, row.4, &row.5, &row.6, &row.7, row.8.as_deref())
    }

    async fn get_by_slug(&self, content_type: &str, slug: &str) -> Result<ContentItem, CmsError> {
        let row = sqlx::query_as::<_, (String, String, String, String, i64, String, String, String, Option<String>)>(
            "SELECT id, content_type, status, fields, version, created_at, updated_at, created_by, scheduled_at
             FROM _cms_content WHERE content_type = ? AND json_extract(fields, '$.slug') = ?"
        )
        .bind(content_type)
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("get_by_slug: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("{content_type}/slug:{slug}")))?;

        row_to_content_item(&row.0, &row.1, &row.2, &row.3, row.4, &row.5, &row.6, &row.7, row.8.as_deref())
    }

    async fn list(&self, content_type: &str, query: ListQuery) -> Result<SearchResult, CmsError> {
        let pagination = query.pagination.clone().unwrap_or_default();
        let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;

        let mut sql = String::from(
            "SELECT id, content_type, status, fields, version, created_at, updated_at, created_by, scheduled_at
             FROM _cms_content WHERE content_type = ?"
        );
        let mut count_sql = String::from("SELECT COUNT(*) FROM _cms_content WHERE content_type = ?");

        if query.status.is_some() {
            sql.push_str(" AND status = ?");
            count_sql.push_str(" AND status = ?");
        }
        sql.push_str(" ORDER BY updated_at DESC LIMIT ? OFFSET ?");

        // Build queries dynamically
        let mut q = sqlx::query_as::<_, (String, String, String, String, i64, String, String, String, Option<String>)>(&sql)
            .bind(content_type);
        let mut count_q = sqlx::query_as::<_, (i64,)>(&count_sql)
            .bind(content_type);

        if let Some(ref status) = query.status {
            q = q.bind(status);
            count_q = count_q.bind(status);
        }

        q = q.bind(pagination.per_page as i64).bind(offset as i64);

        let rows = q.fetch_all(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("list: {e}")))?;

        let total = count_q.fetch_one(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("list count: {e}")))?.0 as u64;

        let items = rows.iter()
            .map(|r| row_to_content_item(&r.0, &r.1, &r.2, &r.3, r.4, &r.5, &r.6, &r.7, r.8.as_deref()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SearchResult { items, total, page: pagination.page, per_page: pagination.per_page })
    }

    async fn create(&self, content_type: &str, data: ContentData, created_by: &str) -> Result<ContentItem, CmsError> {
        let id = new_id();
        let now = now_str();
        let status = data.status.as_deref().unwrap_or("draft").to_string();
        let fields_json = serde_json::to_string(&data.fields)?;
        let scheduled_at = data.scheduled_at.as_ref().map(|dt| dt.to_rfc3339());

        sqlx::query(
            "INSERT INTO _cms_content (id, content_type, status, fields, version, created_at, updated_at, created_by, scheduled_at)
             VALUES (?, ?, ?, ?, 1, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(content_type)
        .bind(&status)
        .bind(&fields_json)
        .bind(&now)
        .bind(&now)
        .bind(created_by)
        .bind(scheduled_at.as_deref())
        .execute(&self.pool)
        .await
        .map_err(|e| CmsError::Storage(format!("create: {e}")))?;

        let item = self.get(content_type, &id).await?;
        self.update_fts(&item).await?;
        info!("created {content_type}/{id}");
        Ok(item)
    }

    async fn update(&self, content_type: &str, id: &str, data: ContentData, updated_by: &str) -> Result<ContentItem, CmsError> {
        let existing = self.get(content_type, id).await?;
        self.snapshot_version(&existing).await?;

        let now = now_str();
        let fields_json = serde_json::to_string(&data.fields)?;
        let status = data.status.as_deref().unwrap_or(&existing.status).to_string();
        let scheduled_at = data.scheduled_at.as_ref().map(|dt| dt.to_rfc3339())
            .or_else(|| existing.scheduled_at.map(|dt| dt.to_rfc3339()));

        sqlx::query(
            "UPDATE _cms_content SET fields = ?, status = ?, updated_at = ?, created_by = ?, scheduled_at = ?, version = version + 1
             WHERE id = ? AND content_type = ?"
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
        self.update_fts(&item).await?;
        Ok(item)
    }

    async fn delete(&self, content_type: &str, id: &str) -> Result<(), CmsError> {
        sqlx::query("DELETE FROM _cms_content WHERE id = ? AND content_type = ?")
            .bind(id).bind(content_type)
            .execute(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("delete: {e}")))?;
        sqlx::query("DELETE FROM _cms_search_fts WHERE id = ?")
            .bind(id).execute(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("fts delete: {e}")))?;
        Ok(())
    }

    async fn transition(&self, content_type: &str, id: &str, to_state: &str, by_user: &str) -> Result<ContentItem, CmsError> {
        let existing = self.get(content_type, id).await?;
        self.snapshot_version(&existing).await?;

        let now = now_str();
        sqlx::query(
            "UPDATE _cms_content SET status = ?, updated_at = ?, created_by = ?, version = version + 1
             WHERE id = ? AND content_type = ?"
        )
        .bind(to_state).bind(&now).bind(by_user).bind(id).bind(content_type)
        .execute(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("transition: {e}")))?;

        self.get(content_type, id).await
    }

    async fn list_versions(&self, content_type: &str, id: &str) -> Result<Vec<ContentVersion>, CmsError> {
        let rows = sqlx::query_as::<_, (String, String, String, i64, String, String, String, String)>(
            "SELECT id, content_id, content_type, version, fields, status, created_by, created_at
             FROM _cms_content_versions WHERE content_id = ? AND content_type = ?
             ORDER BY version DESC"
        )
        .bind(id).bind(content_type)
        .fetch_all(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("list_versions: {e}")))?;

        rows.iter()
            .map(|r| row_to_version(&r.0, &r.1, &r.2, r.3, &r.4, &r.5, &r.6, &r.7))
            .collect()
    }

    async fn restore_version(&self, content_type: &str, id: &str, version: u32, by_user: &str) -> Result<ContentItem, CmsError> {
        let v = sqlx::query_as::<_, (String, String)>(
            "SELECT fields, status FROM _cms_content_versions
             WHERE content_id = ? AND content_type = ? AND version = ?"
        )
        .bind(id).bind(content_type).bind(version as i64)
        .fetch_optional(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("restore_version fetch: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("version {version} for {content_type}/{id}")))?;

        let fields = serde_json::from_str(&v.0)?;
        let data = ContentData { fields, status: Some(v.1), scheduled_at: None };
        self.update(content_type, id, data, by_user).await
    }

    async fn list_scheduled_due(&self, as_of: DateTime<Utc>) -> Result<Vec<ContentItem>, CmsError> {
        let as_of_str = as_of.to_rfc3339();
        let rows = sqlx::query_as::<_, (String, String, String, String, i64, String, String, String, Option<String>)>(
            "SELECT id, content_type, status, fields, version, created_at, updated_at, created_by, scheduled_at
             FROM _cms_content WHERE status = 'approved' AND scheduled_at IS NOT NULL AND scheduled_at <= ?"
        )
        .bind(&as_of_str)
        .fetch_all(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("list_scheduled_due: {e}")))?;

        rows.iter()
            .map(|r| row_to_content_item(&r.0, &r.1, &r.2, &r.3, r.4, &r.5, &r.6, &r.7, r.8.as_deref()))
            .collect()
    }

    async fn search(&self, content_type: Option<&str>, query: &str, pagination: Pagination) -> Result<SearchResult, CmsError> {
        let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;

        let (sql, count_sql, type_bind) = if let Some(ct) = content_type {
            (
                "SELECT c.id, c.content_type, c.status, c.fields, c.version, c.created_at, c.updated_at, c.created_by, c.scheduled_at
                 FROM _cms_search_fts fts JOIN _cms_content c ON fts.id = c.id
                 WHERE fts.content_type = ? AND fts.text_blob MATCH ? ORDER BY rank LIMIT ? OFFSET ?",
                "SELECT COUNT(*) FROM _cms_search_fts WHERE content_type = ? AND text_blob MATCH ?",
                Some(ct.to_string()),
            )
        } else {
            (
                "SELECT c.id, c.content_type, c.status, c.fields, c.version, c.created_at, c.updated_at, c.created_by, c.scheduled_at
                 FROM _cms_search_fts fts JOIN _cms_content c ON fts.id = c.id
                 WHERE fts.text_blob MATCH ? ORDER BY rank LIMIT ? OFFSET ?",
                "SELECT COUNT(*) FROM _cms_search_fts WHERE text_blob MATCH ?",
                None,
            )
        };

        // Type-specific search
        let rows = if let Some(ref ct) = type_bind {
            sqlx::query_as::<_, (String, String, String, String, i64, String, String, String, Option<String>)>(sql)
                .bind(ct).bind(query).bind(pagination.per_page as i64).bind(offset as i64)
                .fetch_all(&self.pool).await
        } else {
            sqlx::query_as::<_, (String, String, String, String, i64, String, String, String, Option<String>)>(sql)
                .bind(query).bind(pagination.per_page as i64).bind(offset as i64)
                .fetch_all(&self.pool).await
        }
        .map_err(|e| CmsError::Storage(format!("search: {e}")))?;

        let total = if let Some(ref ct) = type_bind {
            sqlx::query_as::<_, (i64,)>(count_sql).bind(ct).bind(query)
        } else {
            sqlx::query_as::<_, (i64,)>(count_sql).bind(query)
        }
        .fetch_one(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("search count: {e}")))?.0 as u64;

        let items = rows.iter()
            .map(|r| row_to_content_item(&r.0, &r.1, &r.2, &r.3, r.4, &r.5, &r.6, &r.7, r.8.as_deref()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SearchResult { items, total, page: pagination.page, per_page: pagination.per_page })
    }

    // --- Users ---

    async fn get_user_by_email(&self, email: &str) -> Result<User, CmsError> {
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, String, String)>(
            "SELECT id, email, display_name, bio, avatar_url, roles, created_at FROM _cms_users WHERE email = ?"
        )
        .bind(email)
        .fetch_optional(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("get_user_by_email: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("user:{email}")))?;

        row_to_user(&row.0, &row.1, &row.2, row.3.as_deref(), row.4.as_deref(), &row.5, &row.6)
    }

    async fn get_password_hash(&self, email: &str) -> Result<String, CmsError> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT password_hash FROM _cms_users WHERE email = ?"
        )
        .bind(email)
        .fetch_optional(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("get_password_hash: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("user:{email}")))?;
        Ok(row.0)
    }

    async fn get_user(&self, id: &str) -> Result<User, CmsError> {
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, String, String)>(
            "SELECT id, email, display_name, bio, avatar_url, roles, created_at FROM _cms_users WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("get_user: {e}")))?
        .ok_or_else(|| CmsError::NotFound(format!("user:{id}")))?;

        row_to_user(&row.0, &row.1, &row.2, row.3.as_deref(), row.4.as_deref(), &row.5, &row.6)
    }

    async fn create_user(&self, email: &str, display_name: &str, password_hash: &str, roles: Vec<String>) -> Result<User, CmsError> {
        let id = new_id();
        let now = now_str();
        let roles_json = serde_json::to_string(&roles)?;

        sqlx::query(
            "INSERT INTO _cms_users (id, email, display_name, password_hash, roles, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&id).bind(email).bind(display_name).bind(password_hash).bind(&roles_json).bind(&now)
        .execute(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("create_user: {e}")))?;

        self.get_user(&id).await
    }

    async fn update_user_roles(&self, id: &str, roles: Vec<String>) -> Result<User, CmsError> {
        let roles_json = serde_json::to_string(&roles)?;
        sqlx::query("UPDATE _cms_users SET roles = ? WHERE id = ?")
            .bind(&roles_json).bind(id)
            .execute(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("update_user_roles: {e}")))?;
        self.get_user(id).await
    }

    async fn list_users(&self, pagination: Pagination) -> Result<Vec<User>, CmsError> {
        let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, String, String)>(
            "SELECT id, email, display_name, bio, avatar_url, roles, created_at FROM _cms_users ORDER BY created_at DESC LIMIT ? OFFSET ?"
        )
        .bind(pagination.per_page as i64).bind(offset as i64)
        .fetch_all(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("list_users: {e}")))?;

        rows.iter()
            .map(|r| row_to_user(&r.0, &r.1, &r.2, r.3.as_deref(), r.4.as_deref(), &r.5, &r.6))
            .collect()
    }

    async fn delete_user(&self, id: &str) -> Result<(), CmsError> {
        sqlx::query("DELETE FROM _cms_users WHERE id = ?").bind(id)
            .execute(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("delete_user: {e}")))?;
        Ok(())
    }

    // --- API Tokens ---

    async fn create_api_token(&self, name: &str, user_id: &str, token_hash: &str, scopes: Vec<String>, expires_at: Option<DateTime<Utc>>) -> Result<String, CmsError> {
        let id = new_id();
        let now = now_str();
        let scopes_json = serde_json::to_string(&scopes)?;
        let exp = expires_at.map(|dt| dt.to_rfc3339());

        sqlx::query(
            "INSERT INTO _cms_api_tokens (id, name, token_hash, user_id, scopes, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id).bind(name).bind(token_hash).bind(user_id).bind(&scopes_json).bind(exp.as_deref()).bind(&now)
        .execute(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("create_api_token: {e}")))?;

        Ok(id)
    }

    async fn get_user_by_token_hash(&self, token_hash: &str) -> Result<(User, Vec<String>), CmsError> {
        let now = now_str();
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT user_id, scopes FROM _cms_api_tokens
             WHERE token_hash = ? AND (expires_at IS NULL OR expires_at > ?)"
        )
        .bind(token_hash).bind(&now)
        .fetch_optional(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("get_user_by_token_hash: {e}")))?
        .ok_or(CmsError::Unauthorized)?;

        let scopes: Vec<String> = serde_json::from_str(&row.1)?;
        let user = self.get_user(&row.0).await?;
        Ok((user, scopes))
    }

    async fn revoke_api_token(&self, id: &str) -> Result<(), CmsError> {
        sqlx::query("DELETE FROM _cms_api_tokens WHERE id = ?").bind(id)
            .execute(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("revoke_api_token: {e}")))?;
        Ok(())
    }

    async fn touch_api_token(&self, token_hash: &str) -> Result<(), CmsError> {
        let now = now_str();
        sqlx::query("UPDATE _cms_api_tokens SET last_used = ? WHERE token_hash = ?")
            .bind(&now).bind(token_hash)
            .execute(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("touch_api_token: {e}")))?;
        Ok(())
    }

    // --- Media ---

    async fn create_media(&self, filename: &str, content_type: &str, size: i64, url: &str, uploaded_by: &str) -> Result<MediaItem, CmsError> {
        let id = new_id();
        let now = now_str();

        sqlx::query(
            "INSERT INTO _cms_media (id, filename, content_type, size, url, uploaded_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id).bind(filename).bind(content_type).bind(size).bind(url).bind(uploaded_by).bind(&now)
        .execute(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("create_media: {e}")))?;

        self.get_media(&id).await
    }

    async fn get_media(&self, id: &str) -> Result<MediaItem, CmsError> {
        let row = sqlx::query_as::<_, (String, String, String, i64, String, String, String)>(
            "SELECT id, filename, content_type, size, url, uploaded_by, created_at FROM _cms_media WHERE id = ?"
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
            "SELECT id, filename, content_type, size, url, uploaded_by, created_at FROM _cms_media ORDER BY created_at DESC LIMIT ? OFFSET ?"
        )
        .bind(pagination.per_page as i64).bind(offset as i64)
        .fetch_all(&self.pool).await
        .map_err(|e| CmsError::Storage(format!("list_media: {e}")))?;

        rows.iter()
            .map(|r| row_to_media(&r.0, &r.1, &r.2, r.3, &r.4, &r.5, &r.6))
            .collect()
    }

    async fn delete_media(&self, id: &str) -> Result<(), CmsError> {
        sqlx::query("DELETE FROM _cms_media WHERE id = ?").bind(id)
            .execute(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("delete_media: {e}")))?;
        Ok(())
    }

    // --- Migrations ---

    async fn run_migrations(&self) -> Result<(), CmsError> {
        let migration_sql = std::fs::read_to_string(
            format!("{}/001_base.sql", self.migrations_dir)
        ).map_err(|e| CmsError::Storage(format!("read migration: {e}")))?;

        sqlx::raw_sql(&migration_sql).execute(&self.pool).await
            .map_err(|e| CmsError::Storage(format!("run migration: {e}")))?;

        info!("migrations applied");
        Ok(())
    }
}
