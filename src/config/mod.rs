pub mod content_type;
pub mod global;
pub mod loader;
pub mod role;
pub mod server;
pub mod watcher;
pub mod workflow;

pub use content_type::{ContentTypeConfig, FieldAccess, FieldConfig, FieldType, StorageBackend};
pub use global::{GlobalConfig, GlobalItem, GlobalKind, GlobalStorage};
pub use loader::{CmsConfig, ConfigLoader};
pub use role::{ContentPermissions, FieldPermission, RoleConfig};
pub use server::{
    CacheConfig, ConfigDirs, DatabaseConfig, DatabaseDriver, FlatfileConfig, FlatfileFormat,
    GlobalsServerConfig, MediaConfig, OidcConfig, RateLimitConfig, SecurityConfig, ServerConfig,
    WebhookConfig,
};
pub use watcher::ConfigWatcher;
pub use workflow::{Transition, WorkflowConfig};

#[cfg(test)]
mod tests;
