use super::resolvers::*;
use crate::core::{ContentItem, FieldValue};
use async_graphql::Value;
use chrono::Utc;
use std::collections::HashMap;

#[test]
fn to_camel_basic() {
    assert_eq!(to_camel("foo_bar"), "fooBar");
}

#[test]
fn to_camel_multiple_underscores() {
    assert_eq!(to_camel("seo_description"), "seoDescription");
}

#[test]
fn to_camel_no_underscores() {
    assert_eq!(to_camel("title"), "title");
}

#[test]
fn to_snake_basic() {
    assert_eq!(to_snake("fooBar"), "foo_bar");
}

#[test]
fn to_snake_already_snake() {
    assert_eq!(to_snake("foo_bar"), "foo_bar");
}

#[test]
fn to_snake_no_change_for_lowercase() {
    assert_eq!(to_snake("title"), "title");
}

#[test]
fn to_camel_to_snake_roundtrip() {
    for s in &["foo_bar", "seo_description", "title", "created_at"] {
        assert_eq!(to_snake(&to_camel(s)), *s, "roundtrip failed for {s}");
    }
}

#[test]
fn field_value_to_gql_text() {
    let fv = FieldValue::Text("hello".into());
    assert_eq!(field_value_to_gql(&fv), Value::String("hello".into()));
}

#[test]
fn field_value_to_gql_number_integer() {
    let fv = FieldValue::Number(42.0);
    assert_eq!(field_value_to_gql(&fv), Value::Number(42_i64.into()));
}

#[test]
fn field_value_to_gql_number_float() {
    let fv = FieldValue::Number(3.14);
    match field_value_to_gql(&fv) {
        Value::Number(n) => {
            let f = n.as_f64().unwrap();
            assert!((f - 3.14).abs() < 1e-9);
        }
        other => panic!("expected Number, got {other:?}"),
    }
}

#[test]
fn field_value_to_gql_null() {
    assert_eq!(field_value_to_gql(&FieldValue::Null), Value::Null);
}

#[test]
fn field_value_to_gql_bool() {
    assert_eq!(
        field_value_to_gql(&FieldValue::Bool(true)),
        Value::Boolean(true)
    );
}

#[test]
fn content_item_to_json_includes_all_fields() {
    let mut fields = HashMap::new();
    fields.insert("title".into(), FieldValue::Text("Hello".into()));
    fields.insert("internal_notes".into(), FieldValue::Text("secret".into()));

    let item = ContentItem {
        id: "1".into(),
        content_type: "article".into(),
        status: "draft".into(),
        fields,
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        created_by: "user1".into(),
        updated_by: "user1".into(),
        scheduled_at: None,
    };

    let json = content_item_to_json(&item, &[]);
    let obj = json.as_object().unwrap();
    assert!(obj.contains_key("title"));
    assert!(obj.contains_key("internalNotes")); // camelCase
}

#[test]
fn content_item_to_json_filters_denied_fields() {
    let mut fields = HashMap::new();
    fields.insert("title".into(), FieldValue::Text("Hello".into()));
    fields.insert("internal_notes".into(), FieldValue::Text("secret".into()));

    let item = ContentItem {
        id: "1".into(),
        content_type: "article".into(),
        status: "draft".into(),
        fields,
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        created_by: "user1".into(),
        updated_by: "user1".into(),
        scheduled_at: None,
    };

    let json = content_item_to_json(&item, &["internal_notes".to_string()]);
    let obj = json.as_object().unwrap();
    assert!(obj.contains_key("title"));
    assert!(
        !obj.contains_key("internalNotes"),
        "denied field should be absent"
    );
    assert!(
        !obj.contains_key("internal_notes"),
        "denied field should be absent"
    );
}
