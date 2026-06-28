use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedValue(pub Vec<u8>);

impl CachedValue {
    pub fn from_json<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        Ok(Self(serde_json::to_vec(value)?))
    }

    pub fn to_json<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.0)
    }
}

#[async_trait]
pub trait CacheLayer: Send + Sync {
    async fn get(&self, key: &str) -> Option<CachedValue>;
    async fn set(&self, key: &str, value: CachedValue, ttl: Duration);
    async fn invalidate(&self, key: &str);
    async fn invalidate_prefix(&self, prefix: &str);
}
