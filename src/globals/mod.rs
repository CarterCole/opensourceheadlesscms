//! Storage and access for site-wide globals (see `config::global`).
//!
//! Each global's value lives in one of three backends — the main database, a flat file,
//! or Redis — chosen per server (`[globals] storage`) and optionally per definition.
mod flatfile;
mod redis_store;

pub use flatfile::FlatfileGlobalStore;
pub use redis_store::RedisGlobalStore;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::{CmsConfig, GlobalConfig, GlobalStorage};
use crate::core::error::Result;
use crate::core::{CmsError, ContentStore, StoredGlobal};

/// A place global values are kept. Values are already validated when they get here.
#[async_trait]
pub trait GlobalStore: Send + Sync {
    async fn get(&self, name: &str) -> Result<Option<StoredGlobal>>;
    async fn put(&self, name: &str, value: &StoredGlobal) -> Result<()>;
}

/// `database` backend: the `_cms_globals` table of the main SQLite/PostgreSQL store.
pub struct DatabaseGlobalStore(pub Arc<dyn ContentStore>);

#[async_trait]
impl GlobalStore for DatabaseGlobalStore {
    async fn get(&self, name: &str) -> Result<Option<StoredGlobal>> {
        self.0.get_global(name).await
    }
    async fn put(&self, name: &str, value: &StoredGlobal) -> Result<()> {
        self.0.put_global(name, value).await
    }
}

/// A global's definition joined with its current value.
#[derive(Debug, Clone)]
pub struct ResolvedGlobal<'a> {
    pub def: &'a GlobalConfig,
    pub value: Value,
    pub stored: Option<StoredGlobal>,
}

/// Routes each global to its backend and applies validation on write.
pub struct GlobalsService {
    default_storage: GlobalStorage,
    backends: HashMap<GlobalStorage, Arc<dyn GlobalStore>>,
}

impl GlobalsService {
    pub fn new(default_storage: GlobalStorage) -> Self {
        Self {
            default_storage,
            backends: HashMap::new(),
        }
    }

    /// Database-only service; what `SchemaContext::new` uses until `main` installs the
    /// configured one.
    pub fn database(store: Arc<dyn ContentStore>) -> Self {
        Self::new(GlobalStorage::Database).with_backend(
            GlobalStorage::Database,
            Arc::new(DatabaseGlobalStore(store)),
        )
    }

    pub fn with_backend(mut self, kind: GlobalStorage, store: Arc<dyn GlobalStore>) -> Self {
        self.backends.insert(kind, store);
        self
    }

    /// Build the backends that the configured globals actually use. Fails at startup
    /// rather than on first write when one can't be set up (e.g. Redis unreachable).
    pub async fn from_config(config: &CmsConfig, store: Arc<dyn ContentStore>) -> Result<Self> {
        let g = &config.server.globals;
        let mut svc = Self::new(g.storage);
        let mut needed: Vec<GlobalStorage> = config
            .globals
            .values()
            .map(|d| d.storage.unwrap_or(g.storage))
            .collect();
        needed.push(g.storage);
        needed.sort_by_key(|k| *k as u8);
        needed.dedup();
        for kind in needed {
            let backend: Arc<dyn GlobalStore> = match kind {
                GlobalStorage::Database => Arc::new(DatabaseGlobalStore(store.clone())),
                GlobalStorage::Flatfile => Arc::new(FlatfileGlobalStore::new(
                    &g.flatfile_dir,
                    g.flatfile_format,
                )?),
                GlobalStorage::Redis => {
                    let url = g.redis_url.as_deref().ok_or_else(|| {
                        CmsError::Internal(
                            "globals use redis storage but [globals] redis_url is not set".into(),
                        )
                    })?;
                    Arc::new(RedisGlobalStore::connect(url, &g.redis_key_prefix).await?)
                }
            };
            svc.backends.insert(kind, backend);
        }
        Ok(svc)
    }

    fn backend(&self, def: &GlobalConfig) -> Result<&Arc<dyn GlobalStore>> {
        let kind = def.storage.unwrap_or(self.default_storage);
        self.backends.get(&kind).ok_or_else(|| {
            CmsError::Internal(format!(
                "no {kind:?} backend configured for global `{}`",
                def.name
            ))
        })
    }

    pub async fn read<'a>(&self, def: &'a GlobalConfig) -> Result<ResolvedGlobal<'a>> {
        let stored = self.backend(def)?.get(&def.name).await?;
        let value = stored
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or_else(|| def.empty_value());
        Ok(ResolvedGlobal { def, value, stored })
    }

    /// Validate, normalise and save a value. `Validation` error on a bad value.
    pub async fn write<'a>(
        &self,
        def: &'a GlobalConfig,
        value: Value,
        by_user: &str,
    ) -> Result<ResolvedGlobal<'a>> {
        let value = def.normalize(value).map_err(CmsError::Validation)?;
        let stored = StoredGlobal {
            value: value.clone(),
            updated_at: Some(Utc::now()),
            updated_by: Some(by_user.to_string()),
        };
        self.backend(def)?.put(&def.name, &stored).await?;
        Ok(ResolvedGlobal {
            def,
            value,
            stored: Some(stored),
        })
    }
}

/// Words that can't be a `const` name; such globals are still in the default export.
const JS_RESERVED: &[&str] = &[
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Render globals as an ES module for site builds:
/// `export const primaryNav = {...};` per global plus a default export keyed by slug.
/// JSON is valid JavaScript, so values are emitted with `serde_json`.
pub fn render_js_module(entries: &[(&GlobalConfig, &Value)]) -> String {
    let mut out =
        String::from("// Generated by opensourceheadlesscms from /api/v1/globals — do not edit.\n");
    let mut default_entries = Vec::with_capacity(entries.len());
    for (def, value) in entries {
        let json = serde_json::to_string(value).unwrap_or_else(|_| "null".into());
        let ident = def.js_ident();
        let slug = serde_json::to_string(&def.slug()).unwrap_or_default();
        if JS_RESERVED.contains(&ident.as_str()) {
            default_entries.push(format!("  {slug}: {json},"));
        } else {
            out.push_str(&format!("export const {ident} = {json};\n"));
            default_entries.push(format!("  {slug}: {ident},"));
        }
    }
    out.push_str("export default {\n");
    for line in default_entries {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("};\n");
    out
}
