use crate::config::CmsConfig;
use crate::core::{AllowedFields, CacheLayer, ContentStore, EventBus, PermissionEngine};
use crate::globals::GlobalsService;
use std::sync::Arc;

/// RBAC resource name for globals in role files (`permissions: { globals: { update: true } }`).
pub const GLOBALS_RESOURCE: &str = "globals";

/// Per-request context passed into all GraphQL resolvers
#[derive(Clone)]
pub struct SchemaContext {
    pub store: Arc<dyn ContentStore>,
    pub cache: Arc<dyn CacheLayer>,
    pub rbac: Arc<dyn PermissionEngine>,
    pub config: Arc<CmsConfig>,
    pub events: Arc<EventBus>,
    pub globals: Arc<GlobalsService>,
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
            globals: Arc::new(GlobalsService::database(store.clone())),
            store,
            cache,
            rbac,
            config,
            events,
            user_id: None,
            user_roles: vec![],
        }
    }

    /// Replace the default (database-only) globals backends with the configured ones.
    pub fn with_globals(mut self, globals: Arc<GlobalsService>) -> Self {
        self.globals = globals;
        self
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

    /// Public globals are readable by anyone; others need a signed-in caller with read
    /// access to the `globals` resource.
    pub fn can_read_global(&self, def: &crate::config::GlobalConfig) -> bool {
        def.public
            || (self.is_authenticated() && self.can(crate::core::Action::Read, GLOBALS_RESOURCE))
    }

    /// Saving any global needs update access to the `globals` resource.
    pub fn can_write_globals(&self) -> bool {
        self.is_authenticated() && self.can(crate::core::Action::Update, GLOBALS_RESOURCE)
    }

    pub fn config(&self) -> &CmsConfig {
        &self.config
    }
}
