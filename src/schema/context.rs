use crate::config::CmsConfig;
use crate::core::{AllowedFields, CacheLayer, ContentStore, EventBus, PermissionEngine};
use std::sync::Arc;

/// Per-request context passed into all GraphQL resolvers
#[derive(Clone)]
pub struct SchemaContext {
    pub store: Arc<dyn ContentStore>,
    pub cache: Arc<dyn CacheLayer>,
    pub rbac: Arc<dyn PermissionEngine>,
    pub config: Arc<CmsConfig>,
    pub events: Arc<EventBus>,
    pub user_id: Option<String>,
    pub user_roles: Vec<String>,
}

impl SchemaContext {
    pub fn new(
        store: Arc<dyn ContentStore>,
        cache: Arc<dyn CacheLayer>,
        rbac: Arc<dyn PermissionEngine>,
        config: Arc<CmsConfig>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            store,
            cache,
            rbac,
            config,
            events,
            user_id: None,
            user_roles: vec![],
        }
    }

    pub fn with_user(mut self, user_id: String, roles: Vec<String>) -> Self {
        self.user_id = Some(user_id);
        self.user_roles = roles;
        self
    }

    pub fn is_authenticated(&self) -> bool {
        self.user_id.is_some()
    }

    pub fn allowed_fields(&self, content_type: &str, action: crate::core::Action) -> AllowedFields {
        self.rbac
            .allowed_fields(&self.user_roles, content_type, action)
    }

    pub fn can(&self, action: crate::core::Action, content_type: &str) -> bool {
        self.rbac.can(&self.user_roles, action, content_type)
    }

    pub fn config(&self) -> &CmsConfig {
        &self.config
    }
}
