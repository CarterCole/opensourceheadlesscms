#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Read,
    Update,
    Delete,
    Transition,
}

#[derive(Debug, Clone)]
pub enum AllowedFields {
    All,
    Only(Vec<String>),
    Except(Vec<String>),
}

impl AllowedFields {
    pub fn allows(&self, field: &str) -> bool {
        match self {
            AllowedFields::All => true,
            AllowedFields::Only(fields) => fields.iter().any(|f| f == field),
            AllowedFields::Except(fields) => !fields.iter().any(|f| f == field),
        }
    }
}

pub trait PermissionEngine: Send + Sync {
    fn can(&self, roles: &[String], action: Action, content_type: &str) -> bool;
    fn allowed_fields(&self, roles: &[String], content_type: &str, action: Action) -> AllowedFields;
    fn can_transition(&self, roles: &[String], content_type: &str, from_state: &str, to_state: &str) -> bool;
}
