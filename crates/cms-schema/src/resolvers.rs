use async_graphql::Value;
use cms_core::{ContentItem, FieldValue};
use std::collections::HashMap;

/// snake_case → camelCase
pub fn to_camel(s: &str) -> String {
    let mut result = String::new();
    let mut cap_next = false;
    for c in s.chars() {
        if c == '_' {
            cap_next = true;
        } else if cap_next {
            result.push(c.to_uppercase().next().unwrap_or(c));
            cap_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

/// camelCase → snake_case
pub fn to_snake(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if c.is_uppercase() {
            result.push('_');
            result.push(c.to_lowercase().next().unwrap_or(c));
        } else {
            result.push(c);
        }
    }
    result
}

/// Convert FieldValue to GraphQL Value
pub fn field_value_to_gql(fv: &FieldValue) -> Value {
    match fv {
        FieldValue::Text(s) => Value::String(s.clone()),
        FieldValue::Number(n) => {
            if n.fract() == 0.0 {
                Value::Number((*n as i64).into())
            } else {
                Value::Number(
                    serde_json::Number::from_f64(*n)
                        .unwrap_or(serde_json::Number::from(0))
                        .into(),
                )
            }
        }
        FieldValue::Bool(b) => Value::Boolean(*b),
        FieldValue::Date(dt) => Value::String(dt.to_rfc3339()),
        FieldValue::Blocks(blocks) => {
            let json = serde_json::to_value(blocks).unwrap_or_default();
            json_to_gql(&json)
        }
        FieldValue::Relation(ids) => Value::List(ids.iter().map(|id| Value::String(id.clone())).collect()),
        FieldValue::List(items) => Value::List(items.iter().map(field_value_to_gql).collect()),
        FieldValue::Null => Value::Null,
    }
}

pub fn json_to_gql(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Boolean(*b),
        serde_json::Value::Number(n) => Value::Number(n.clone().into()),
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::List(arr.iter().map(json_to_gql).collect()),
        serde_json::Value::Object(obj) => {
            Value::Object(obj.iter().map(|(k, v)| (async_graphql::Name::new(k), json_to_gql(v))).collect())
        }
    }
}

/// Serialize a ContentItem to a flat JSON object for use as parent_value in dynamic resolvers.
/// RBAC filtering is applied: denied fields are omitted.
pub fn content_item_to_json(item: &ContentItem, denied_fields: &[String]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("id".to_string(), serde_json::Value::String(item.id.clone()));
    map.insert("status".to_string(), serde_json::Value::String(item.status.clone()));
    map.insert("version".to_string(), serde_json::Value::Number(item.version.into()));
    map.insert("createdAt".to_string(), serde_json::Value::String(item.created_at.to_rfc3339()));
    map.insert("updatedAt".to_string(), serde_json::Value::String(item.updated_at.to_rfc3339()));
    map.insert("createdBy".to_string(), serde_json::Value::String(item.created_by.clone()));

    for (field_name, field_val) in &item.fields {
        if !denied_fields.contains(field_name) {
            let gql_key = to_camel(field_name);
            let json_val = field_value_to_json(field_val);
            map.insert(gql_key, json_val);
        }
    }

    serde_json::Value::Object(map)
}

fn field_value_to_json(fv: &FieldValue) -> serde_json::Value {
    match fv {
        FieldValue::Text(s) => serde_json::Value::String(s.clone()),
        FieldValue::Number(n) => serde_json::to_value(n).unwrap_or(serde_json::Value::Null),
        FieldValue::Bool(b) => serde_json::Value::Bool(*b),
        FieldValue::Date(dt) => serde_json::Value::String(dt.to_rfc3339()),
        FieldValue::Blocks(b) => serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        FieldValue::Relation(ids) => serde_json::to_value(ids).unwrap_or(serde_json::Value::Null),
        FieldValue::List(items) => {
            serde_json::Value::Array(items.iter().map(field_value_to_json).collect())
        }
        FieldValue::Null => serde_json::Value::Null,
    }
}

/// Parse a GraphQL input Value into ContentData fields
pub fn input_to_fields(input: &Value) -> (HashMap<String, FieldValue>, Option<String>, Option<chrono::DateTime<chrono::Utc>>) {
    let mut fields = HashMap::new();
    let mut status = None;
    let mut scheduled_at = None;

    if let Value::Object(obj) = input {
        for (key, val) in obj {
            let key_str = key.as_str();
            match key_str {
                "status" => {
                    if let Value::String(s) = val { status = Some(s.clone()); }
                }
                "scheduledAt" => {
                    if let Value::String(s) = val {
                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                            scheduled_at = Some(dt.with_timezone(&chrono::Utc));
                        }
                    }
                }
                _ => {
                    let field_name = to_snake(key_str);
                    fields.insert(field_name, gql_to_field_value(val));
                }
            }
        }
    }

    (fields, status, scheduled_at)
}

fn gql_to_field_value(val: &Value) -> FieldValue {
    match val {
        Value::String(s) => FieldValue::Text(s.clone()),
        Value::Number(n) => FieldValue::Number(n.as_f64().unwrap_or(0.0)),
        Value::Boolean(b) => FieldValue::Bool(*b),
        Value::List(items) => FieldValue::List(items.iter().map(gql_to_field_value).collect()),
        Value::Null => FieldValue::Null,
        other => FieldValue::Text(serde_json::to_string(other).unwrap_or_default()),
    }
}
