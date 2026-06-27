use async_trait::async_trait;
use cms_core::{CacheLayer, CachedValue};
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

pub struct MokaCache {
    inner: Cache<String, CachedValue>,
}

impl MokaCache {
    pub fn new(max_capacity: u64, default_ttl: Duration) -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(max_capacity)
                .time_to_live(default_ttl)
                .build(),
        }
    }
}

#[async_trait]
impl CacheLayer for MokaCache {
    async fn get(&self, key: &str) -> Option<CachedValue> {
        let v = self.inner.get(key).await;
        if v.is_some() {
            debug!(key, "cache L1 hit");
        }
        v
    }

    async fn set(&self, key: &str, value: CachedValue, _ttl: Duration) {
        self.inner.insert(key.to_string(), value).await;
    }

    async fn invalidate(&self, key: &str) {
        self.inner.invalidate(key).await;
        debug!(key, "cache invalidated");
    }

    async fn invalidate_prefix(&self, prefix: &str) {
        let keys: Vec<String> = self.inner.iter()
            .map(|(k, _)| k.as_ref().clone())
            .filter(|k| k.starts_with(prefix))
            .collect();
        for key in keys {
            self.inner.invalidate(&key).await;
        }
        debug!(prefix, "cache prefix invalidated");
    }
}

/// Two-layer cache: L1 (in-process Moka) + optional L2 (Redis, added later)
pub struct TwoLayerCache {
    l1: Arc<MokaCache>,
}

impl TwoLayerCache {
    pub fn new(l1: MokaCache) -> Self {
        Self { l1: Arc::new(l1) }
    }
}

#[async_trait]
impl CacheLayer for TwoLayerCache {
    async fn get(&self, key: &str) -> Option<CachedValue> {
        self.l1.get(key).await
    }

    async fn set(&self, key: &str, value: CachedValue, ttl: Duration) {
        self.l1.set(key, value, ttl).await;
    }

    async fn invalidate(&self, key: &str) {
        self.l1.invalidate(key).await;
    }

    async fn invalidate_prefix(&self, prefix: &str) {
        self.l1.invalidate_prefix(prefix).await;
    }
}

pub fn cache_key_id(content_type: &str, id: &str) -> String {
    format!("{content_type}:{id}")
}

pub fn cache_key_slug(content_type: &str, slug: &str) -> String {
    format!("{content_type}:slug:{slug}")
}
