use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use tracing::{info, warn};

use crate::config::{ContentTypeConfig, RoleConfig, ServerConfig, WorkflowConfig};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml parse error in {file}: {source}")]
    Yaml { file: String, source: serde_yaml::Error },
    #[error("toml parse error: {0}")]
    Toml(String),
    #[error("missing required config: {0}")]
    Missing(String),
}

#[derive(Debug, Clone)]
pub struct CmsConfig {
    pub server: ServerConfig,
    pub content_types: HashMap<String, ContentTypeConfig>,
    pub workflows: HashMap<String, WorkflowConfig>,
    pub roles: HashMap<String, RoleConfig>,
}

impl CmsConfig {
    pub fn content_type(&self, name: &str) -> Option<&ContentTypeConfig> {
        self.content_types.get(name)
    }

    pub fn workflow(&self, name: &str) -> Option<&WorkflowConfig> {
        self.workflows.get(name)
    }

    pub fn workflow_for_type(&self, ct: &ContentTypeConfig) -> Option<&WorkflowConfig> {
        ct.workflow.as_deref().and_then(|w| self.workflows.get(w))
    }

    pub fn role(&self, name: &str) -> Option<&RoleConfig> {
        self.roles.get(name)
    }
}

pub struct ConfigLoader {
    config_path: String,
}

impl ConfigLoader {
    pub fn new(config_path: impl Into<String>) -> Self {
        Self { config_path: config_path.into() }
    }

    pub fn load(&self) -> Result<CmsConfig, ConfigError> {
        let server = self.load_server_config()?;

        let content_types = load_yaml_dir::<ContentTypeConfig>(&server.config_dirs.content_types)?
            .into_iter().map(|ct| (ct.name.clone(), ct)).collect();

        let workflows = load_yaml_dir::<WorkflowConfig>(&server.config_dirs.workflows)?
            .into_iter().map(|wf| (wf.name.clone(), wf)).collect();

        let roles = load_yaml_dir::<RoleConfig>(&server.config_dirs.roles)?
            .into_iter().map(|r| (r.name.clone(), r)).collect();

        Ok(CmsConfig { server, content_types, workflows, roles })
    }

    fn load_server_config(&self) -> Result<ServerConfig, ConfigError> {
        let content = std::fs::read_to_string(&self.config_path)?;
        toml::from_str::<ServerConfig>(&content)
            .map_err(|e| ConfigError::Toml(e.to_string()))
    }
}

fn load_yaml_dir<T: for<'de> serde::Deserialize<'de>>(dir: &str) -> Result<Vec<T>, ConfigError> {
    let path = Path::new(dir);
    if !path.exists() {
        warn!("config dir not found, skipping: {dir}");
        return Ok(vec![]);
    }

    let mut items = vec![];
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let file_path = entry.path();
        if file_path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let content = std::fs::read_to_string(&file_path)?;
        let item: T = serde_yaml::from_str(&content).map_err(|e| ConfigError::Yaml {
            file: file_path.display().to_string(),
            source: e,
        })?;
        info!("loaded config: {}", file_path.display());
        items.push(item);
    }
    Ok(items)
}
