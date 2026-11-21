use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use opensourceheadlesscms::core::{ContentData, ContentStore, FieldValue, ListQuery, Pagination};
use opensourceheadlesscms::db::SqliteStore;
use rand_core::OsRng;
use std::collections::HashMap;

async fn make_store() -> SqliteStore {
    let store = SqliteStore::new("sqlite://:memory:", "migrations/sqlite")
        .await
        .expect("create in-memory store");
    store.run_migrations().await.expect("run migrations");
    store
}

fn hash_password(pw: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn article_data(title: &str) -> ContentData {
    let mut fields = HashMap::new();
    fields.insert("title".into(), FieldValue::Text(title.into()));
    fields.insert(
        "slug".into(),
        FieldValue::Text(title.to_lowercase().replace(' ', "-")),
    );
    ContentData {
        fields,
        status: None,
        scheduled_at: None,
    }
}

#[tokio::test]
async fn create_and_get_content() {
    let store = make_store().await;
    let item = store
        .create("article", article_data("Hello World"), "user1")
        .await
        .unwrap();
    let fetched = store.get("article", &item.id).await.unwrap();
    assert_eq!(fetched.id, item.id);
    assert_eq!(
        fetched.fields["title"],
        FieldValue::Text("Hello World".into())
    );
}

#[tokio::test]
async fn create_and_get_by_slug() {
    let store = make_store().await;
    store
        .create("article", article_data("Slug Test"), "user1")
        .await
        .unwrap();
    let fetched = store.get_by_slug("article", "slug-test").await.unwrap();
    assert_eq!(
        fetched.fields["title"],
        FieldValue::Text("Slug Test".into())
    );
}

#[tokio::test]
async fn list_filters_by_status() {
    let store = make_store().await;

    let draft = store
        .create("article", article_data("Draft"), "user1")
        .await
        .unwrap();
    let to_publish = store
        .create("article", article_data("Published"), "user1")
        .await
        .unwrap();
    store
        .transition("article", &to_publish.id, "published", "user1")
        .await
        .unwrap();

    let query = ListQuery {
        status: Some("published".into()),
        pagination: Some(Pagination::default()),
        ..Default::default()
    };
    let result = store.list("article", query).await.unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].id, to_publish.id);
    let _ = draft; // suppress unused warning
}

#[tokio::test]
async fn update_increments_version() {
    let store = make_store().await;
    let item = store
        .create("article", article_data("Original"), "user1")
        .await
        .unwrap();
    assert_eq!(item.version, 1);

    let mut updated_data = article_data("Updated");
    updated_data
        .fields
        .insert("title".into(), FieldValue::Text("Updated".into()));
    let updated = store
        .update("article", &item.id, updated_data, "user1")
        .await
        .unwrap();
    assert_eq!(updated.version, 2);
}

#[tokio::test]
async fn delete_removes_item() {
    let store = make_store().await;
    let item = store
        .create("article", article_data("To Delete"), "user1")
        .await
        .unwrap();
    store.delete("article", &item.id).await.unwrap();

    let result = store.get("article", &item.id).await;
    assert!(result.is_err(), "item should no longer exist after delete");
}

#[tokio::test]
async fn transition_changes_status() {
    let store = make_store().await;
    let item = store
        .create("article", article_data("Transitioning"), "user1")
        .await
        .unwrap();
    assert_eq!(item.status, "draft");

    let transitioned = store
        .transition("article", &item.id, "published", "user1")
        .await
        .unwrap();
    assert_eq!(transitioned.status, "published");
}

#[tokio::test]
async fn list_versions_after_updates() {
    let store = make_store().await;
    let item = store
        .create("article", article_data("V1"), "user1")
        .await
        .unwrap();

    let mut d2 = article_data("V2");
    d2.fields
        .insert("title".into(), FieldValue::Text("V2".into()));
    store
        .update("article", &item.id, d2, "user1")
        .await
        .unwrap();

    let mut d3 = article_data("V3");
    d3.fields
        .insert("title".into(), FieldValue::Text("V3".into()));
    store
        .update("article", &item.id, d3, "user1")
        .await
        .unwrap();

    // `update` snapshots the *previous* state, so two updates leave two history rows
    // (V1 and V2); the live row (V3) is not part of the history.
    let versions = store.list_versions("article", &item.id).await.unwrap();
    assert_eq!(
        versions.len(),
        2,
        "expected 2 historical versions, got {}",
        versions.len()
    );
}

#[tokio::test]
async fn restore_version_reverts_fields() {
    let store = make_store().await;
    let item = store
        .create("article", article_data("Original Title"), "user1")
        .await
        .unwrap();

    let mut d2 = article_data("Changed Title");
    d2.fields
        .insert("title".into(), FieldValue::Text("Changed Title".into()));
    store
        .update("article", &item.id, d2, "user1")
        .await
        .unwrap();

    // Restore version 1
    store
        .restore_version("article", &item.id, 1, "user1")
        .await
        .unwrap();
    let current = store.get("article", &item.id).await.unwrap();
    assert_eq!(
        current.fields["title"],
        FieldValue::Text("Original Title".into())
    );
}

#[tokio::test]
async fn create_user_and_get_password_hash() {
    let store = make_store().await;
    let hash = hash_password("secret123");
    store
        .create_user("alice@example.com", "Alice", &hash, vec!["editor".into()])
        .await
        .unwrap();

    let stored_hash = store.get_password_hash("alice@example.com").await.unwrap();
    assert_eq!(stored_hash, hash);
}

#[tokio::test]
async fn fts_search_finds_content() {
    let store = make_store().await;
    store
        .create("article", article_data("Rust programming guide"), "user1")
        .await
        .unwrap();
    store
        .create("article", article_data("Python tutorial"), "user1")
        .await
        .unwrap();

    let result = store
        .search(Some("article"), "Rust", Pagination::default())
        .await
        .unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0].fields["title"],
        FieldValue::Text("Rust programming guide".into())
    );
}

#[tokio::test]
async fn api_token_create_and_lookup() {
    let store = make_store().await;
    let hash = hash_password("pass");
    let user = store
        .create_user("bob@example.com", "Bob", &hash, vec!["admin".into()])
        .await
        .unwrap();

    // Use a pre-computed SHA-256 hex string (no sha2 dep needed)
    let token_hash = "a3f1b2c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2".to_string();
    let scopes = vec!["content:read".into()];

    store
        .create_api_token("ci-token", &user.id, &token_hash, scopes.clone(), None)
        .await
        .unwrap();

    let (found_user, found_scopes) = store.get_user_by_token_hash(&token_hash).await.unwrap();
    assert_eq!(found_user.email, "bob@example.com");
    assert_eq!(found_scopes, scopes);
}
