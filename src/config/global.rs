//! Site-wide "globals": named values that aren't content items — a site title, a list of
//! announcement strings, a set of keywords, a map of nav labels to URLs, a map of theme
//! tokens to JSON objects. Each is defined by a YAML file in `config_dirs.globals`.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Shape of a global's value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GlobalKind {
    /// One item.
    Single,
    /// Ordered collection; editors choose the order, duplicates allowed.
    List,
    /// Unordered collection of unique items (stored de-duplicated and sorted).
    Set,
    /// Ordered map of labels (keys) to items; keys are unique.
    Map,
}

/// Type of each item (the value of `single`, the elements of `list`/`set`, the values of `map`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum GlobalItem {
    #[default]
    Text,
    /// `http(s)://`, `mailto:`, `tel:` or a site-relative path (`/about`, `#top`).
    Url,
    /// Any JSON value except null.
    Json,
}

/// Where a global's value is kept. Server default is `[globals] storage` in `cms.toml`;
/// a definition's own `storage:` overrides it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum GlobalStorage {
    /// The main database (SQLite or PostgreSQL, per `[database]`).
    #[default]
    Database,
    /// One file per global under `[globals] flatfile_dir`, in `flatfile_format`.
    Flatfile,
    /// A Redis (or Redis-compatible) key per global.
    Redis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub name: String,
    /// URL-safe identifier used in API paths (`/api/v1/globals/{slug}`) and as the key in
    /// exports. Defaults to `name` with `_` replaced by `-`.
    pub slug: Option<String>,
    pub label: Option<String>,
    pub description: Option<String>,
    pub kind: GlobalKind,
    #[serde(default)]
    pub item: GlobalItem,
    /// Served to anonymous callers by `/api/v1/globals`. `false` = signed-in readers only.
    #[serde(default = "default_true")]
    pub public: bool,
    /// Overrides the server-wide default backend.
    pub storage: Option<GlobalStorage>,
    /// Value used until one is saved.
    pub default: Option<Value>,
    /// Cap on entries for list/set/map (default 500).
    pub max_items: Option<usize>,
}

fn default_true() -> bool {
    true
}

const DEFAULT_MAX_ITEMS: usize = 500;
const MAX_TEXT_BYTES: usize = 100_000;
const MAX_KEY_BYTES: usize = 200;
/// Upper bound on a whole serialized value.
pub const MAX_VALUE_BYTES: usize = 256 * 1024;

impl GlobalConfig {
    pub fn slug(&self) -> String {
        self.slug
            .clone()
            .unwrap_or_else(|| self.name.replace('_', "-"))
    }

    /// camelCase identifier for the slug, used as the export name in generated JS.
    pub fn js_ident(&self) -> String {
        let mut out = String::new();
        let mut upper = false;
        for c in self.slug().chars() {
            if c == '-' {
                upper = true;
            } else if upper {
                out.push(c.to_ascii_uppercase());
                upper = false;
            } else {
                out.push(c);
            }
        }
        out
    }

    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }

    /// Value served before anything is saved: the configured default, else an empty value.
    pub fn empty_value(&self) -> Value {
        if let Some(d) = &self.default {
            return d.clone();
        }
        match (self.kind, self.item) {
            (GlobalKind::Single, GlobalItem::Json) => Value::Null,
            (GlobalKind::Single, _) => Value::String(String::new()),
            (GlobalKind::List | GlobalKind::Set, _) => Value::Array(vec![]),
            (GlobalKind::Map, _) => Value::Object(Default::default()),
        }
    }

    /// Check a submitted value against the definition and return its stored form
    /// (strings trimmed, sets de-duplicated and sorted).
    pub fn normalize(&self, value: Value) -> Result<Value, String> {
        let max = self.max_items.unwrap_or(DEFAULT_MAX_ITEMS);
        let out = match self.kind {
            GlobalKind::Single => self.normalize_item(value, "value")?,
            GlobalKind::List | GlobalKind::Set => {
                let Value::Array(items) = value else {
                    return Err("expected an array".into());
                };
                if items.len() > max {
                    return Err(format!("at most {max} items allowed"));
                }
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.into_iter().enumerate() {
                    out.push(self.normalize_item(v, &format!("item {}", i + 1))?);
                }
                if self.kind == GlobalKind::Set {
                    let mut keyed: Vec<(String, Value)> =
                        out.into_iter().map(|v| (sort_key(&v), v)).collect();
                    keyed.sort_by(|a, b| a.0.cmp(&b.0));
                    keyed.dedup_by(|a, b| a.0 == b.0);
                    out = keyed.into_iter().map(|(_, v)| v).collect();
                }
                Value::Array(out)
            }
            GlobalKind::Map => {
                let Value::Object(entries) = value else {
                    return Err("expected an object of label → value".into());
                };
                if entries.len() > max {
                    return Err(format!("at most {max} entries allowed"));
                }
                let mut out = serde_json::Map::new();
                for (k, v) in entries {
                    let key = k.trim().to_string();
                    if key.is_empty() {
                        return Err("map labels must not be empty".into());
                    }
                    if key.len() > MAX_KEY_BYTES {
                        return Err(format!("label `{key}` is too long"));
                    }
                    if out.contains_key(&key) {
                        return Err(format!("duplicate label `{key}`"));
                    }
                    let v = self.normalize_item(v, &format!("`{key}`"))?;
                    out.insert(key, v);
                }
                Value::Object(out)
            }
        };
        if serde_json::to_string(&out).map(|s| s.len()).unwrap_or(0) > MAX_VALUE_BYTES {
            return Err("value is too large".into());
        }
        Ok(out)
    }

    fn normalize_item(&self, v: Value, what: &str) -> Result<Value, String> {
        match self.item {
            GlobalItem::Text => match v {
                // Leading/trailing whitespace is never meaningful here, and trimming lets
                // sets treat "rust" and "rust " as the same item.
                Value::String(s) if s.len() <= MAX_TEXT_BYTES => {
                    Ok(Value::String(s.trim().to_string()))
                }
                Value::String(_) => Err(format!("{what} is too long")),
                _ => Err(format!("{what} must be a string")),
            },
            GlobalItem::Url => match v {
                Value::String(s) => {
                    let s = s.trim().to_string();
                    if is_allowed_url(&s) {
                        Ok(Value::String(s))
                    } else {
                        Err(format!(
                            "{what} must be an http(s), mailto: or tel: URL, or a path starting with /"
                        ))
                    }
                }
                _ => Err(format!("{what} must be a URL string")),
            },
            GlobalItem::Json => match v {
                Value::Null => Err(format!("{what} must not be null")),
                v => Ok(v),
            },
        }
    }

    /// Load-time checks: a usable name and a default that passes validation.
    pub fn check(&self) -> Result<(), String> {
        if !is_valid_name(&self.name) {
            return Err(format!(
                "global name `{}` must be lowercase letters, digits and _ (starting with a letter, max 64)",
                self.name
            ));
        }
        if !is_valid_slug(&self.slug()) {
            return Err(format!(
                "global `{}` slug `{}` must be lowercase letters and digits joined by single hyphens, starting with a letter (max 64)",
                self.name,
                self.slug()
            ));
        }
        if let Some(d) = &self.default {
            self.normalize(d.clone())
                .map_err(|e| format!("global `{}` default: {e}", self.name))?;
        }
        Ok(())
    }
}

fn sort_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 64
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// URL-safe slug that also maps to a valid JS identifier: `site-nav`, `footer-links2`.
pub fn is_valid_slug(slug: &str) -> bool {
    slug.len() <= 64
        && slug.starts_with(|c: char| c.is_ascii_lowercase())
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Links that are safe to drop into an `href`: no `javascript:`/`data:`, no
/// protocol-relative `//host`.
fn is_allowed_url(s: &str) -> bool {
    if s.is_empty() || s.len() > 2048 || s.chars().any(|c| c.is_control() || c == ' ') {
        return false;
    }
    if s.starts_with('#') || (s.starts_with('/') && !s.starts_with("//") && !s.starts_with("/\\")) {
        return true;
    }
    match url::Url::parse(s) {
        Ok(u) => match u.scheme() {
            "http" | "https" => u.host().is_some(),
            "mailto" | "tel" => true,
            _ => false,
        },
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn def(kind: GlobalKind, item: GlobalItem) -> GlobalConfig {
        GlobalConfig {
            name: "g".into(),
            slug: None,
            label: None,
            description: None,
            kind,
            item,
            public: true,
            storage: None,
            default: None,
            max_items: Some(3),
        }
    }

    #[test]
    fn single_text() {
        let d = def(GlobalKind::Single, GlobalItem::Text);
        assert_eq!(d.normalize(json!("Hi")).unwrap(), json!("Hi"));
        assert_eq!(
            d.normalize(json!("  two\nlines \n")).unwrap(),
            json!("two\nlines")
        );
        assert!(d.normalize(json!(3)).is_err());
        assert_eq!(d.empty_value(), json!(""));
    }

    #[test]
    fn list_keeps_order_and_duplicates() {
        let d = def(GlobalKind::List, GlobalItem::Text);
        assert_eq!(
            d.normalize(json!(["b", "a", "b"])).unwrap(),
            json!(["b", "a", "b"])
        );
        assert!(d.normalize(json!(["a", "b", "c", "d"])).is_err());
        assert!(d.normalize(json!("a")).is_err());
    }

    #[test]
    fn set_dedupes_and_sorts() {
        let d = def(GlobalKind::Set, GlobalItem::Text);
        assert_eq!(
            d.normalize(json!(["b", "a", "b"])).unwrap(),
            json!(["a", "b"])
        );
    }

    #[test]
    fn map_of_urls_keeps_order_and_rejects_bad_links() {
        let d = def(GlobalKind::Map, GlobalItem::Url);
        let v = d
            .normalize(json!({"Home": "/", "Blog": " https://example.com/blog ", "Mail": "mailto:a@example.com"}))
            .unwrap();
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            r#"{"Home":"/","Blog":"https://example.com/blog","Mail":"mailto:a@example.com"}"#
        );
        for bad in [
            "javascript:alert(1)",
            "//evil.example",
            "data:text/html,x",
            "ftp://x",
            "",
        ] {
            assert!(d.normalize(json!({ "x": bad })).is_err(), "{bad} accepted");
        }
        assert!(d.normalize(json!({ " ": "/" })).is_err());
        assert!(d.normalize(json!({ "a": "/", " a": "/x" })).is_err());
    }

    #[test]
    fn map_of_json() {
        let d = def(GlobalKind::Map, GlobalItem::Json);
        assert!(d.normalize(json!({"primary": {"color": "#000"}})).is_ok());
        assert!(d.normalize(json!({"primary": null})).is_err());
    }

    #[test]
    fn check_validates_name_and_default() {
        let mut d = def(GlobalKind::Set, GlobalItem::Text);
        d.name = "Bad-Name".into();
        assert!(d.check().is_err());
        d.name = "keywords".into();
        d.default = Some(json!([1]));
        assert!(d.check().is_err());
        d.default = Some(json!(["rust"]));
        assert!(d.check().is_ok());
    }

    #[test]
    fn slug_defaults_from_name_and_is_validated() {
        let mut d = def(GlobalKind::Map, GlobalItem::Url);
        d.name = "primary_nav".into();
        assert_eq!(d.slug(), "primary-nav");
        assert_eq!(d.js_ident(), "primaryNav");
        assert!(d.check().is_ok());
        for bad in ["Nav", "-nav", "nav-", "na--v", "9nav", "na_v", "na v"] {
            d.slug = Some(bad.into());
            assert!(d.check().is_err(), "{bad} accepted");
        }
        d.slug = Some("main-nav2".into());
        assert!(d.check().is_ok());
        assert_eq!(d.js_ident(), "mainNav2");
    }
}
