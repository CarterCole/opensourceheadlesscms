use chrono::{DateTime, Utc};
use crate::core::{CmsError, ContentItem, ContentVersion, FieldValue, MediaItem, User};
use std::collections::HashMap;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().hyphenated().to_string()
}

pub fn now_str() -> String {
    Utc::now().to_rfc3339()
}

pub fn parse_dt(s: &str) -> Result<DateTime<Utc>, CmsError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| CmsError::Internal(format!("date parse error: {e}")))
}

pub fn row_to_content_item(
    id: &str,
    content_type: &str,
    status: &str,
    fields_json: &str,
    version: i64,
    created_at: &str,
    updated_at: &str,
    created_by: &str,
    scheduled_at: Option<&str>,
) -> Result<ContentItem, CmsError> {
    let fields: HashMap<String, FieldValue> = serde_json::from_str(fields_json)
        .map_err(|e| CmsError::Storage(format!("fields deserialize: {e}")))?;

    Ok(ContentItem {
        id: id.to_string(),
        content_type: content_type.to_string(),
        status: status.to_string(),
        fields,
        version: version as u32,
        created_at: parse_dt(created_at)?,
        updated_at: parse_dt(updated_at)?,
        created_by: created_by.to_string(),
        scheduled_at: scheduled_at.map(parse_dt).transpose()?,
    })
}

pub fn row_to_user(
    id: &str,
    email: &str,
    display_name: &str,
    bio: Option<&str>,
    avatar_url: Option<&str>,
    roles_json: &str,
    created_at: &str,
) -> Result<User, CmsError> {
    let roles: Vec<String> = serde_json::from_str(roles_json)
        .map_err(|e| CmsError::Storage(format!("roles deserialize: {e}")))?;
    Ok(User {
        id: id.to_string(),
        email: email.to_string(),
        display_name: display_name.to_string(),
        bio: bio.map(|s| s.to_string()),
        avatar_url: avatar_url.map(|s| s.to_string()),
        roles,
        created_at: parse_dt(created_at)?,
    })
}

pub fn row_to_media(
    id: &str,
    filename: &str,
    content_type: &str,
    size: i64,
    url: &str,
    uploaded_by: &str,
    created_at: &str,
) -> Result<MediaItem, CmsError> {
    Ok(MediaItem {
        id: id.to_string(),
        filename: filename.to_string(),
        content_type: content_type.to_string(),
        size,
        url: url.to_string(),
        uploaded_by: uploaded_by.to_string(),
        created_at: parse_dt(created_at)?,
    })
}

pub fn row_to_version(
    id: &str,
    content_id: &str,
    content_type: &str,
    version: i64,
    fields_json: &str,
    status: &str,
    created_by: &str,
    created_at: &str,
) -> Result<ContentVersion, CmsError> {
    let fields: HashMap<String, FieldValue> = serde_json::from_str(fields_json)
        .map_err(|e| CmsError::Storage(format!("version fields deserialize: {e}")))?;
    Ok(ContentVersion {
        id: id.to_string(),
        content_id: content_id.to_string(),
        content_type: content_type.to_string(),
        version: version as u32,
        fields,
        status: status.to_string(),
        created_by: created_by.to_string(),
        created_at: parse_dt(created_at)?,
    })
}
