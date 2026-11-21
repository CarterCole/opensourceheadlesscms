use crate::config::{CmsConfig, RoleConfig};
use crate::core::{Action, AllowedFields, PermissionEngine};
use std::collections::HashMap;

/// `access:` restriction declared on a field in the content-type YAML.
struct FieldRule {
    field: String,
    read: Vec<String>,
    write: Vec<String>,
}

pub struct RbacEngine {
    roles: HashMap<String, RoleConfig>,
    workflow_transitions: HashMap<String, Vec<(String, String, Vec<String>)>>,
    /// content_type → field-level `access` rules from the content-type definitions
    field_rules: HashMap<String, Vec<FieldRule>>,
}

impl RbacEngine {
    pub fn new(config: &CmsConfig) -> Self {
        let roles: HashMap<String, RoleConfig> = config.roles.clone();

        // Pre-index: content_type → [(from, to, roles)]
        let mut workflow_transitions: HashMap<String, Vec<(String, String, Vec<String>)>> =
            HashMap::new();
        let mut field_rules: HashMap<String, Vec<FieldRule>> = HashMap::new();
        for ct in config.content_types.values() {
            let rules: Vec<FieldRule> = ct
                .fields
                .iter()
                .filter_map(|f| {
                    f.access.as_ref().map(|a| FieldRule {
                        field: f.name.clone(),
                        read: a.read.clone(),
                        write: a.write.clone(),
                    })
                })
                .collect();
            if !rules.is_empty() {
                field_rules.insert(ct.name.clone(), rules);
            }
            if let Some(wf_name) = &ct.workflow {
                if let Some(wf) = config.workflows.get(wf_name) {
                    let transitions = wf
                        .transitions
                        .iter()
                        .map(|t| (t.from.clone(), t.to.clone(), t.roles.clone()))
                        .collect();
                    workflow_transitions.insert(ct.name.clone(), transitions);
                }
            }
        }

        Self {
            roles,
            workflow_transitions,
            field_rules,
        }
    }

    fn effective_permissions<'a>(
        &'a self,
        roles: &[String],
        content_type: &str,
    ) -> Vec<&'a crate::config::ContentPermissions> {
        roles
            .iter()
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

    fn allowed_fields(
        &self,
        roles: &[String],
        content_type: &str,
        action: Action,
    ) -> AllowedFields {
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

        // Field-level `access` from the content-type definition. A non-empty list is an
        // allow-list of roles; callers holding none of them are denied (fail closed).
        if let Some(rules) = self.field_rules.get(content_type) {
            for rule in rules {
                let allow = match action {
                    Action::Read => &rule.read,
                    Action::Create | Action::Update => &rule.write,
                    _ => continue,
                };
                if !allow.is_empty()
                    && !allow.iter().any(|r| roles.contains(r))
                    && !denied.contains(&rule.field)
                {
                    denied.push(rule.field.clone());
                }
            }
        }

        if denied.is_empty() {
            AllowedFields::All
        } else {
            AllowedFields::Except(denied)
        }
    }

    fn can_transition(
        &self,
        roles: &[String],
        content_type: &str,
        from_state: &str,
        to_state: &str,
    ) -> bool {
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
        use crate::config::{
            CacheConfig, CmsConfig, ConfigDirs, DatabaseConfig, DatabaseDriver, FlatfileConfig,
            MediaConfig, RateLimitConfig, ServerConfig, WebhookConfig,
        };
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
                webhooks: vec![
                    WebhookConfig {
                        url: String::new(),
                        events: vec![],
                        secret: None,
                        content_types: vec![]
                    };
                    0
                ],
                rate_limit: RateLimitConfig::default(),
                media: MediaConfig::default(),
                security: Default::default(),
                oidc: None,
            },
            content_types: HashMap::new(),
            workflows: HashMap::new(),
            roles: roles.into_iter().map(|r| (r.name.clone(), r)).collect(),
        }
    }

    #[test]
    fn editor_cannot_read_restricted_field() {
        let mut fields = HashMap::new();
        fields.insert(
            "internal_notes".into(),
            FieldPermission {
                read: false,
                write: false,
            },
        );

        let mut permissions = HashMap::new();
        permissions.insert(
            "article".into(),
            ContentPermissions {
                create: true,
                read: true,
                update: true,
                delete: false,
                fields,
            },
        );

        let role = RoleConfig {
            name: "editor".into(),
            permissions,
        };
        let config = make_config_with_roles(vec![role]);
        let engine = RbacEngine::new(&config);

        let allowed = engine.allowed_fields(&["editor".to_string()], "article", Action::Read);
        assert!(!allowed.allows("internal_notes"));
        assert!(allowed.allows("title"));
    }

    #[test]
    fn admin_wildcard_can_do_everything() {
        let mut permissions = HashMap::new();
        permissions.insert(
            "*".into(),
            ContentPermissions {
                create: true,
                read: true,
                update: true,
                delete: true,
                fields: HashMap::new(),
            },
        );

        let role = RoleConfig {
            name: "admin".into(),
            permissions,
        };
        let config = make_config_with_roles(vec![role]);
        let engine = RbacEngine::new(&config);

        assert!(engine.can(&["admin".to_string()], Action::Delete, "article"));
        assert!(engine.can(&["admin".to_string()], Action::Create, "product"));
    }

    #[test]
    fn no_matching_role_denies_all() {
        let config = make_config_with_roles(vec![]);
        let engine = RbacEngine::new(&config);
        assert!(!engine.can(&["unknown_role".to_string()], Action::Read, "article"));
    }

    #[test]
    fn multiple_roles_union_permissions() {
        let mut editor_perms = HashMap::new();
        editor_perms.insert(
            "article".into(),
            ContentPermissions {
                create: true,
                read: true,
                update: false,
                delete: false,
                fields: HashMap::new(),
            },
        );
        let editor = RoleConfig {
            name: "editor".into(),
            permissions: editor_perms,
        };

        let mut admin_perms = HashMap::new();
        admin_perms.insert(
            "article".into(),
            ContentPermissions {
                create: true,
                read: true,
                update: true,
                delete: true,
                fields: HashMap::new(),
            },
        );
        let admin = RoleConfig {
            name: "admin".into(),
            permissions: admin_perms,
        };

        let config = make_config_with_roles(vec![editor, admin]);
        let engine = RbacEngine::new(&config);

        let roles = vec!["editor".to_string(), "admin".to_string()];
        assert!(engine.can(&roles, Action::Delete, "article"));
        assert!(engine.can(&roles, Action::Create, "article"));
    }

    #[test]
    fn can_transition_allowed_and_blocked() {
        use crate::config::{ContentTypeConfig, StorageBackend, Transition, WorkflowConfig};

        let wf = WorkflowConfig {
            name: "editorial".into(),
            states: vec!["draft".into(), "in_review".into(), "published".into()],
            transitions: vec![
                Transition {
                    from: "draft".into(),
                    to: "in_review".into(),
                    roles: vec!["editor".into()],
                    trigger: None,
                },
                Transition {
                    from: "in_review".into(),
                    to: "published".into(),
                    roles: vec!["admin".into()],
                    trigger: None,
                },
            ],
        };

        let ct = ContentTypeConfig {
            name: "article".into(),
            label: Some("Article".into()),
            storage: StorageBackend::Database,
            fields: vec![],
            workflow: Some("editorial".into()),
            searchable: false,
        };

        let mut config = make_config_with_roles(vec![]);
        config.workflows.insert("editorial".into(), wf);
        config.content_types.insert("article".into(), ct);

        let mut editor_perms = HashMap::new();
        editor_perms.insert(
            "article".into(),
            ContentPermissions {
                create: true,
                read: true,
                update: true,
                delete: false,
                fields: HashMap::new(),
            },
        );
        config.roles.insert(
            "editor".into(),
            RoleConfig {
                name: "editor".into(),
                permissions: editor_perms,
            },
        );

        let engine = RbacEngine::new(&config);
        let editor_roles = vec!["editor".to_string()];

        assert!(engine.can_transition(&editor_roles, "article", "draft", "in_review"));
        assert!(!engine.can_transition(&editor_roles, "article", "in_review", "published"));
    }
}
