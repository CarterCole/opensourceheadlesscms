use async_graphql::{dynamic::*, Value};
use crate::core::FieldValue;
use serde_json::Value as JsonValue;

/// Convert a FieldValue to an async-graphql dynamic Value
pub fn field_value_to_gql(fv: &FieldValue) -> Value {
    match fv {
        FieldValue::Text(s) => Value::String(s.clone()),
        FieldValue::Number(n) => {
            if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                Value::Number((*n as i64).into())
            } else {
                Value::Number(serde_json::Number::from_f64(*n)
                    .unwrap_or(serde_json::Number::from(0))
                    .into())
            }
        }
        FieldValue::Bool(b) => Value::Boolean(*b),
        FieldValue::Date(dt) => Value::String(dt.to_rfc3339()),
        FieldValue::Blocks(blocks) => {
            let json = serde_json::to_value(blocks).unwrap_or(JsonValue::Array(vec![]));
            json_to_gql_value(&json)
        }
        FieldValue::Relation(ids) => {
            Value::List(ids.iter().map(|id| Value::String(id.clone())).collect())
        }
        FieldValue::List(items) => {
            Value::List(items.iter().map(field_value_to_gql).collect())
        }
        FieldValue::Null => Value::Null,
    }
}

pub fn json_to_gql_value(json: &JsonValue) -> Value {
    match json {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Boolean(*b),
        JsonValue::Number(n) => Value::Number(n.clone().into()),
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(arr) => Value::List(arr.iter().map(json_to_gql_value).collect()),
        JsonValue::Object(obj) => {
            Value::Object(obj.iter().map(|(k, v)| (async_graphql::Name::new(k), json_to_gql_value(v))).collect())
        }
    }
}
