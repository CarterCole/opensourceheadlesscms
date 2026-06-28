use crate::config::{CmsConfig, RoleConfig};
use crate::core::{Action, AllowedFields, PermissionEngine};
use std::collections::HashMap;

pub struct RbacEngine {
    roles: HashMap<String, RoleConfig>,
    workflow_transitions: HashMap<String, Vec<(String, String, Vec<String>)>>,
}

impl RbacEngine {
    pub fn new(config: &CmsConfig) -> Self {
        let roles: HashMap<String, RoleConfig> = config.roles.clone();

        // Pre-index: content_type → [(from, to, roles)]
        let mut workflow_transitions: HashMap<String, Vec<(String, String, Vec<String>)>> = HashMap::new();
        for ct in config.content_types.values() {
            if let Some(wf_name) = &ct.workflow {
                if let Some(wf) = config.workflows.get(wf_name) {
                    let transitions = wf.transitions.iter()
                        .map(|t| (t.from.clone(), t.to.clone(), t.roles.clone()))
                        .collect();
                    workflow_transitions.insert(ct.name.clone(), transitions);
                }
            }
        }

        Self { roles, workflow_transitions }
    }

    fn effective_permissions<'a>(&'a self, roles: &[String], content_type: &str)
        -> Vec<&'a crate::config::ContentPermissions>
    {
        roles.iter()
            .filter_map(|r| self.roles.get(r))
            .filter_map(|rc| rc.permissions_for(content_type))
            .collect()
    }
}

impl PermissionEngine for RbacEngine {
    fn can(&self, roles: &[String], action: Action, content_type: &str) -> bool {
        // Super-admin bypass: if any role is "admin" with wildcard "*", allow all
        let perms = self.effective_permissions(roles, content_type);
        if perms.is_empty() {
            return false;
        }
        perms.iter().any(|p| match action {
            Action::Create => p.create,
            Action::Read => p.read,
            Action::Update => p.update,
            Action::Delete => p.delete,
            Action::Transition => p.update, // transitions require update permission
        })
    }

    fn allowed_fields(&self, roles: &[String], content_type: &str, action: Action) -> AllowedFields {
        let perms = self.effective_permissions(roles, content_type);

        // Collect all denied fields across applicable role permissions
        let mut denied: Vec<String> = vec![];
        for perm in &perms {
            for (field, fp) in &perm.fields {
                let allowed = match action {
                    Action::Read => fp.read,
                    Action::Create | Action::Update => fp.write,
                    _ => true,
                };
                if !allowed && !denied.contains(field) {
                    denied.push(field.clone());
                }
            }
        }

        if denied.is_empty() {
            AllowedFields::All
        } else {
            AllowedFields::Except(denied)
        }
    }

    fn can_transition(&self, roles: &[String], content_type: &str, from_state: &str, to_state: &str) -> bool {
        let Some(transitions) = self.workflow_transitions.get(content_type) else {
            // No workflow defined — allow transition for any role with update permission
            return self.can(roles, Action::Update, content_type);
        };

        transitions.iter().any(|(from, to, allowed_roles)| {
            from == from_state
                && to == to_state
                && (allowed_roles.is_empty() || allowed_roles.iter().any(|r| roles.contains(r)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ContentPermissions, FieldPermission, RoleConfig};
    use std::collections::HashMap;

    fn make_config_with_roles(roles: Vec<RoleConfig>) -> CmsConfig {
        use crate::config::{CmsConfig, ConfigDirs, DatabaseConfig, DatabaseDriver, ServerConfig, CacheConfig, FlatfileConfig, RateLimitConfig, MediaConfig, WebhookConfig};
        CmsConfig {
            server: ServerConfig {
                http_port: 8080,
                grpc_port: 50051,
                jwt_secret: "test".into(),
                env: "test".into(),
                database: DatabaseConfig {
                    driver: DatabaseDriver::Sqlite,
                    url: Some("sqlite://:memory:".into()),
                    auto_migrate: false,
                    cf_account_id: None,
                    cf_database_id: None,
                    cf_api_token: None,
                },
                cache: CacheConfig::default(),
                flatfile: FlatfileConfig::default(),
                config_dirs: ConfigDirs::default(),
                webhooks: vec![WebhookConfig { url: String::new(), events: vec![], secret: None, content_types: vec![] }; 0],
                rate_limit: RateLimitConfig::default(),
                media: MediaConfig::default(),
            },
            content_types: HashMap::new(),
            workflows: HashMap::new(),
            roles: roles.into_iter().map(|r| (r.name.clone(), r)).collect(),
        }
    }

    #[test]
    fn editor_cannot_read_restricted_field() {
        let mut fields = HashMap::new();
        fields.insert("internal_notes".into(), FieldPermission { read: false, write: false });

        let mut permissions = HashMap::new();
        permissions.insert("article".into(), ContentPermissions {
            create: true, read: true, update: true, delete: false, fields,
        });

        let role = RoleConfig { name: "editor".into(), permissions };
        let config = make_config_with_roles(vec![role]);
        let engine = RbacEngine::new(&config);

        let allowed = engine.allowed_fields(&["editor".to_string()], "article", Action::Read);
        assert!(!allowed.allows("internal_notes"));
        assert!(allowed.allows("title"));
    }

    #[test]
    fn admin_wildcard_can_do_everything() {
        let mut permissions = HashMap::new();
        permissions.insert("*".into(), ContentPermissions {
            create: true, read: true, update: true, delete: true, fields: HashMap::new(),
        });

        let role = RoleConfig { name: "admin".into(), permissions };
        let config = make_config_with_roles(vec![role]);
        let engine = RbacEngine::new(&config);

        assert!(engine.can(&["admin".to_string()], Action::Delete, "article"));
        assert!(engine.can(&["admin".to_string()], Action::Create, "product"));
    }
}
