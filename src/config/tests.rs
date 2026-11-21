use super::*;

fn editorial_workflow() -> WorkflowConfig {
    serde_yaml::from_str(
        r#"
name: editorial
states: [draft, in_review, approved, published, archived]
transitions:
  - from: draft
    to: in_review
    roles: [editor, admin]
  - from: in_review
    to: approved
    roles: [senior_editor, admin]
  - from: approved
    to: published
    roles: [admin]
    trigger: scheduled_at
  - from: published
    to: archived
"#,
    )
    .unwrap()
}

#[test]
fn workflow_initial_state() {
    let wf = editorial_workflow();
    assert_eq!(wf.initial_state(), "draft");
}

#[test]
fn workflow_is_valid_state_known() {
    let wf = editorial_workflow();
    assert!(wf.is_valid_state("draft"));
    assert!(wf.is_valid_state("published"));
}

#[test]
fn workflow_is_valid_state_unknown() {
    let wf = editorial_workflow();
    assert!(!wf.is_valid_state("bogus"));
}

#[test]
fn workflow_can_transition_allowed_role() {
    let wf = editorial_workflow();
    let roles = vec!["editor".to_string()];
    assert!(wf.can_transition("draft", "in_review", &roles));
}

#[test]
fn workflow_can_transition_blocked_role() {
    let wf = editorial_workflow();
    let roles = vec!["editor".to_string()];
    assert!(!wf.can_transition("in_review", "approved", &roles));
}

#[test]
fn workflow_can_transition_roleless() {
    let wf = editorial_workflow();
    // published→archived has no role restriction
    let no_roles: Vec<String> = vec![];
    assert!(wf.can_transition("published", "archived", &no_roles));
}

#[test]
fn workflow_allowed_transitions_for_editor() {
    let wf = editorial_workflow();
    let roles = vec!["editor".to_string()];
    let transitions = wf.allowed_transitions("draft", &roles);
    assert_eq!(transitions, vec!["in_review"]);
}

#[test]
fn workflow_allowed_transitions_empty_when_blocked() {
    let wf = editorial_workflow();
    let roles = vec!["editor".to_string()];
    let transitions = wf.allowed_transitions("in_review", &roles);
    assert!(transitions.is_empty());
}

#[test]
fn workflow_scheduled_transitions() {
    let wf = editorial_workflow();
    let scheduled = wf.scheduled_transitions();
    // Only approved→published has trigger
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0], ("approved", "published"));
}

#[test]
fn role_permissions_for_wildcard() {
    let role: RoleConfig = serde_yaml::from_str(
        r#"
name: admin
permissions:
  "*":
    create: true
    read: true
    update: true
    delete: true
"#,
    )
    .unwrap();

    let perms = role.permissions_for("article").unwrap();
    assert!(perms.create);
    assert!(perms.delete);
}

#[test]
fn role_permissions_for_specific_beats_wildcard() {
    let role: RoleConfig = serde_yaml::from_str(
        r#"
name: editor
permissions:
  "*":
    read: true
    create: false
  article:
    read: true
    create: true
"#,
    )
    .unwrap();

    let perms = role.permissions_for("article").unwrap();
    assert!(perms.create, "specific entry should override wildcard");

    let perms_other = role.permissions_for("product").unwrap();
    assert!(
        !perms_other.create,
        "wildcard applies when no specific entry"
    );
}

#[test]
fn role_permissions_for_missing_type_falls_back_to_wildcard() {
    let role: RoleConfig = serde_yaml::from_str(
        r#"
name: viewer
permissions:
  "*":
    read: true
"#,
    )
    .unwrap();

    assert!(role.permissions_for("anything").is_some());
}

#[test]
fn role_permissions_for_returns_none_when_no_match() {
    let role: RoleConfig = serde_yaml::from_str(
        r#"
name: restricted
permissions:
  article:
    read: true
"#,
    )
    .unwrap();

    assert!(role.permissions_for("product").is_none());
}

#[test]
fn config_loader_missing_file_returns_err() {
    let loader = ConfigLoader::new("/nonexistent/cms.toml");
    assert!(loader.load().is_err());
}

#[test]
fn workflow_empty_states_initial_returns_draft() {
    let wf = WorkflowConfig {
        name: "empty".into(),
        states: vec![],
        transitions: vec![],
    };
    assert_eq!(wf.initial_state(), "draft");
}

#[test]
fn globals_with_clashing_graphql_names_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.yaml"),
        "name: a\nslug: a-b2\nkind: single\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.yaml"),
        "name: b\nslug: a-b-2\nkind: single\n",
    )
    .unwrap();
    let err = crate::config::loader::load_globals(dir.path().to_str().unwrap()).unwrap_err();
    assert!(err.to_string().contains("aB2"), "{err}");
}
