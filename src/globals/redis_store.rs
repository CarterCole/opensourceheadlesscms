use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

use super::GlobalStore;
use crate::core::error::Result;
use crate::core::{CmsError, StoredGlobal};

/// `redis` backend: one string key per global, `<prefix><name>`, holding the JSON-encoded
/// `{value, updated_at, updated_by}`. Works with any Redis-protocol store (Valkey,
/// KeyDB, Dragonfly, Upstash…). Keys never expire.
pub struct RedisGlobalStore {
    conn: ConnectionManager,
    prefix: String,
}

fn storage_err(e: redis::RedisError) -> CmsError {
    CmsError::Storage(format!("redis: {e}"))
}

impl RedisGlobalStore {
    pub async fn connect(url: &str, prefix: &str) -> Result<Self> {
        let client = redis::Client::open(url).map_err(storage_err)?;
        let conn = ConnectionManager::new(client).await.map_err(storage_err)?;
        Ok(Self {
            conn,
            prefix: prefix.to_string(),
        })
    }

    fn key(&self, name: &str) -> String {
        format!("{}{name}", self.prefix)
    }
}

#[async_trait]
impl GlobalStore for RedisGlobalStore {
    async fn get(&self, name: &str) -> Result<Option<StoredGlobal>> {
        let mut conn = self.conn.clone();
        let raw: Option<String> = conn.get(self.key(name)).await.map_err(storage_err)?;
        raw.map(|s| serde_json::from_str(&s).map_err(CmsError::from))
            .transpose()
    }

    async fn put(&self, name: &str, value: &StoredGlobal) -> Result<()> {
        let mut conn = self.conn.clone();
        let raw = serde_json::to_string(value)?;
        let _: () = conn.set(self.key(name), raw).await.map_err(storage_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Runs only when `CMS_TEST_REDIS_URL` points at a disposable Redis.
    #[tokio::test]
    async fn round_trip_against_live_redis() {
        let Ok(url) = std::env::var("CMS_TEST_REDIS_URL") else {
            eprintln!("skipping: CMS_TEST_REDIS_URL not set");
            return;
        };
        let prefix = format!("cms-test:{}:", uuid::Uuid::new_v4());
        let store = RedisGlobalStore::connect(&url, &prefix).await.unwrap();
        assert_eq!(store.get("nav").await.unwrap(), None);
        let v = StoredGlobal {
            value: json!({"B": "/b", "A": "/a"}),
            updated_at: None,
            updated_by: Some("u".into()),
        };
        store.put("nav", &v).await.unwrap();
        let back = store.get("nav").await.unwrap().unwrap();
        assert_eq!(back, v);
        assert_eq!(
            back.value.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["B", "A"]
        );
        let mut conn = store.conn.clone();
        let _: () = conn.del(store.key("nav")).await.unwrap();
    }
}
