use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

use super::GlobalStore;
use crate::config::global::is_valid_name;
use crate::config::FlatfileFormat;
use crate::core::error::Result;
use crate::core::{CmsError, StoredGlobal};

/// `flatfile` backend: `<dir>/<name>.<json|yaml|toml>`, holding `value` plus optional
/// `updated_at` / `updated_by`. Files may be edited by hand (or kept in git); they are
/// re-read on every request that misses the cache.
pub struct FlatfileGlobalStore {
    dir: PathBuf,
    format: FlatfileFormat,
    write_lock: Mutex<()>,
}

impl FlatfileGlobalStore {
    pub fn new(dir: impl AsRef<Path>, format: FlatfileFormat) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)
            .map_err(|e| CmsError::Storage(format!("globals dir {}: {e}", dir.display())))?;
        Ok(Self {
            dir,
            format,
            write_lock: Mutex::new(()),
        })
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        // Names come from definitions, but never let one escape the directory.
        if !is_valid_name(name) {
            return Err(CmsError::Validation(format!("bad global name `{name}`")));
        }
        Ok(self.dir.join(format!("{name}.{}", self.format.extension())))
    }

    fn encode(&self, value: &StoredGlobal) -> Result<String> {
        match self.format {
            FlatfileFormat::Json => Ok(serde_json::to_string_pretty(value)? + "\n"),
            FlatfileFormat::Yaml => serde_yaml::to_string(value)
                .map_err(|e| CmsError::Storage(format!("yaml encode: {e}"))),
            FlatfileFormat::Toml => toml::to_string_pretty(value).map_err(|e| {
                // TOML has no null; a JSON item containing one can't be written.
                CmsError::Validation(format!("value can't be stored as TOML: {e}"))
            }),
        }
    }

    fn decode(&self, text: &str, path: &Path) -> Result<StoredGlobal> {
        let bad = |e: String| CmsError::Storage(format!("{}: {e}", path.display()));
        match self.format {
            FlatfileFormat::Json => serde_json::from_str(text).map_err(|e| bad(e.to_string())),
            FlatfileFormat::Yaml => serde_yaml::from_str(text).map_err(|e| bad(e.to_string())),
            FlatfileFormat::Toml => toml::from_str(text).map_err(|e| bad(e.to_string())),
        }
    }
}

#[async_trait]
impl GlobalStore for FlatfileGlobalStore {
    async fn get(&self, name: &str) -> Result<Option<StoredGlobal>> {
        let path = self.path(name)?;
        match tokio::fs::read_to_string(&path).await {
            Ok(text) => self.decode(&text, &path).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CmsError::Storage(format!("{}: {e}", path.display()))),
        }
    }

    async fn put(&self, name: &str, value: &StoredGlobal) -> Result<()> {
        let path = self.path(name)?;
        let text = self.encode(value)?;
        let _guard = self.write_lock.lock().await;
        // Write a sibling temp file and rename over the target so readers never see a
        // half-written file.
        let tmp = path.with_extension(format!("{}.tmp", self.format.extension()));
        tokio::fs::write(&tmp, text)
            .await
            .map_err(|e| CmsError::Storage(format!("{}: {e}", tmp.display())))?;
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(|e| CmsError::Storage(format!("{}: {e}", path.display())))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn round_trips_every_format() {
        for format in [
            FlatfileFormat::Json,
            FlatfileFormat::Yaml,
            FlatfileFormat::Toml,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let store = FlatfileGlobalStore::new(dir.path(), format).unwrap();
            assert_eq!(store.get("nav").await.unwrap(), None);
            let v = StoredGlobal {
                value: json!({"Home": "/", "Blog": "/blog", "theme": {"dark": true, "n": [1, 2]}}),
                updated_at: Some(chrono::Utc::now()),
                updated_by: Some("u1".into()),
            };
            store.put("nav", &v).await.unwrap();
            let back = store.get("nav").await.unwrap().unwrap();
            assert_eq!(back.value, v.value, "{format:?}");
            // map order survives the file format
            assert_eq!(
                back.value.as_object().unwrap().keys().collect::<Vec<_>>(),
                ["Home", "Blog", "theme"],
                "{format:?}"
            );
            assert!(dir
                .path()
                .join(format!("nav.{}", format.extension()))
                .exists());
        }
    }

    #[tokio::test]
    async fn reads_hand_written_file_without_metadata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("keywords.yaml"),
            "value:\n  - rust\n  - cms\n",
        )
        .unwrap();
        let store = FlatfileGlobalStore::new(dir.path(), FlatfileFormat::Yaml).unwrap();
        let got = store.get("keywords").await.unwrap().unwrap();
        assert_eq!(got.value, json!(["rust", "cms"]));
        assert_eq!(got.updated_by, None);
    }

    #[tokio::test]
    async fn toml_rejects_null() {
        let dir = tempfile::tempdir().unwrap();
        let store = FlatfileGlobalStore::new(dir.path(), FlatfileFormat::Toml).unwrap();
        let v = StoredGlobal {
            value: json!({"a": [null]}),
            updated_at: None,
            updated_by: None,
        };
        assert!(matches!(
            store.put("x", &v).await,
            Err(CmsError::Validation(_))
        ));
    }

    #[test]
    fn refuses_path_traversal_names() {
        let dir = tempfile::tempdir().unwrap();
        let store = FlatfileGlobalStore::new(dir.path(), FlatfileFormat::Json).unwrap();
        assert!(store.path("../etc/passwd").is_err());
    }
}
