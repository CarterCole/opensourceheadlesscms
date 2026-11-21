use super::*;
use crate::core::{CacheLayer, CachedValue};
use std::time::Duration;

#[tokio::test]
async fn moka_set_then_get() {
    let cache = MokaCache::new(100, Duration::from_secs(60));
    let val = CachedValue::from_json(&"hello world").unwrap();
    cache
        .set("key1", val.clone(), Duration::from_secs(60))
        .await;

    let got = cache.get("key1").await.expect("should be present");
    let s: String = got.to_json().unwrap();
    assert_eq!(s, "hello world");
}

#[tokio::test]
async fn moka_invalidate_removes_key() {
    let cache = MokaCache::new(100, Duration::from_secs(60));
    let val = CachedValue::from_json(&42u32).unwrap();
    cache.set("key2", val, Duration::from_secs(60)).await;
    cache.invalidate("key2").await;
    assert!(cache.get("key2").await.is_none());
}

#[tokio::test]
async fn moka_invalidate_prefix_removes_matching() {
    let cache = MokaCache::new(100, Duration::from_secs(60));
    let val = CachedValue::from_json(&true).unwrap();

    cache
        .set("article:123", val.clone(), Duration::from_secs(60))
        .await;
    cache
        .set("article:slug:foo", val.clone(), Duration::from_secs(60))
        .await;
    cache
        .set("product:456", val.clone(), Duration::from_secs(60))
        .await;

    cache.invalidate_prefix("article:").await;

    assert!(
        cache.get("article:123").await.is_none(),
        "article:123 should be evicted"
    );
    assert!(
        cache.get("article:slug:foo").await.is_none(),
        "article:slug:foo should be evicted"
    );
    assert!(
        cache.get("product:456").await.is_some(),
        "product:456 should remain"
    );
}

#[tokio::test]
async fn cached_value_json_roundtrip() {
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Payload {
        id: u32,
        name: String,
    }

    let original = Payload {
        id: 1,
        name: "test".into(),
    };
    let cached = CachedValue::from_json(&original).unwrap();
    let recovered: Payload = cached.to_json().unwrap();
    assert_eq!(recovered, original);
}
