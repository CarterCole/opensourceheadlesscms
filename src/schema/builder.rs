use async_graphql::{dynamic::*, Value};
use crate::config::{CmsConfig, ContentTypeConfig};
use crate::core::{Action, ContentData, ListQuery, Pagination};

use crate::schema::context::SchemaContext;
use crate::schema::resolvers::{content_item_to_json, input_to_fields, json_to_gql, to_camel};

pub fn build_schema(config: &CmsConfig, ctx_template: SchemaContext) -> Result<Schema, SchemaError> {
    let mut query = Object::new("Query");
    let mut mutation = Object::new("Mutation");

    let mut extra_types: Vec<Object> = vec![];
    let mut input_types: Vec<InputObject> = vec![];

    for ct in config.content_types.values() {
        let (obj, input_obj, q_fields, m_fields) = build_type(ct, &ctx_template);

        extra_types.push(obj);
        input_types.push(input_obj);

        for f in q_fields { query = query.field(f); }
        for f in m_fields { mutation = mutation.field(f); }
    }

    // Full-text search
    let ctx_s = ctx_template.clone();
    query = query.field(
        Field::new("search", TypeRef::named_nn_list_nn("SearchResult"), move |ctx| {
            let ctx_s = ctx_s.clone();
            FieldFuture::new(async move {
                let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
                let q = ctx.args.try_get("query")?.string()?.to_string();
                let ct = ctx.args.try_get("contentType").ok()
                    .and_then(|v| v.string().ok().map(|s| s.to_string()));
                let page = ctx.args.try_get("page").ok()
                    .and_then(|v| v.u64().ok()).unwrap_or(1) as u32;

                let result = gql_ctx.store.search(ct.as_deref(), &q, Pagination { page, per_page: 20 })
                    .await.map_err(|e| async_graphql::Error::new(e.to_string()))?;

                let items: Vec<FieldValue> = result.items.iter().map(|item| {
                    let denied = get_denied(gql_ctx, &item.content_type);
                    let json = content_item_to_json(item, &denied);
                    FieldValue::value(json_to_gql(&json))
                }).collect();
                Ok(Some(FieldValue::list(items)))
            })
        })
        .argument(InputValue::new("query", TypeRef::named_nn(TypeRef::STRING)))
        .argument(InputValue::new("contentType", TypeRef::named(TypeRef::STRING)))
        .argument(InputValue::new("page", TypeRef::named(TypeRef::INT))),
    );

    let search_result = Object::new("SearchResult")
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), extract_json_field("id")))
        .field(Field::new("contentType", TypeRef::named_nn(TypeRef::STRING), extract_json_field("contentType")))
        .field(Field::new("status", TypeRef::named_nn(TypeRef::STRING), extract_json_field("status")));

    let mut builder = Schema::build("Query", Some("Mutation"), None)
        .register(Scalar::new("JSON"))
        .register(Scalar::new("DateTime"))
        .register(search_result)
        .register(query)
        .register(mutation)
        .data(ctx_template);

    for t in extra_types { builder = builder.register(t); }
    for t in input_types { builder = builder.register(t); }

    builder.finish()
}

fn get_denied(ctx: &SchemaContext, content_type: &str) -> Vec<String> {
    match ctx.rbac.allowed_fields(&ctx.user_roles, content_type, Action::Read) {
        crate::core::AllowedFields::Except(denied) => denied,
        _ => vec![],
    }
}

/// Build output Object, InputObject, query fields, and mutation fields for one content type
fn build_type(ct: &ContentTypeConfig, ctx_tmpl: &SchemaContext) -> (Object, InputObject, Vec<Field>, Vec<Field>) {
    let type_name = pascal(&ct.name);
    let input_name = format!("{type_name}Input");
    let camel = to_camel(&ct.name);
    let list_name = format!("{camel}s");

    // ---- Output type ----
    let mut obj = Object::new(&type_name)
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), extract_json_field("id")))
        .field(Field::new("status", TypeRef::named_nn(TypeRef::STRING), extract_json_field("status")))
        .field(Field::new("version", TypeRef::named_nn(TypeRef::INT), extract_json_field("version")))
        .field(Field::new("createdAt", TypeRef::named_nn("DateTime"), extract_json_field("createdAt")))
        .field(Field::new("updatedAt", TypeRef::named_nn("DateTime"), extract_json_field("updatedAt")))
        .field(Field::new("createdBy", TypeRef::named_nn(TypeRef::STRING), extract_json_field("createdBy")));

    for field in &ct.fields {
        let gql_name = to_camel(&field.name);
        obj = obj.field(Field::new(&gql_name, TypeRef::named("JSON"), extract_json_field_owned(gql_name.clone())));
    }

    // ---- Input type ----
    let mut input_obj = InputObject::new(&input_name)
        .field(InputValue::new("status", TypeRef::named(TypeRef::STRING)))
        .field(InputValue::new("scheduledAt", TypeRef::named("DateTime")));
    for field in &ct.fields {
        let gql_name = to_camel(&field.name);
        input_obj = input_obj.field(InputValue::new(&gql_name, TypeRef::named("JSON")));
    }

    // ---- Query: single ----
    let ct_q = ct.name.clone();
    let ctx_q = ctx_tmpl.clone();
    let single = Field::new(&camel, TypeRef::named(type_name.clone()), move |ctx| {
        let ct_q = ct_q.clone();
        let _ctx_q = ctx_q.clone();
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            if !gql_ctx.can(Action::Read, &ct_q) {
                return Err(async_graphql::Error::new("forbidden"));
            }
            let denied = get_denied(gql_ctx, &ct_q);
            let item = if let Ok(acc) = ctx.args.try_get("id") {
                gql_ctx.store.get(&ct_q, acc.string()?).await
            } else if let Ok(acc) = ctx.args.try_get("slug") {
                gql_ctx.store.get_by_slug(&ct_q, acc.string()?).await
            } else {
                return Err(async_graphql::Error::new("provide id or slug"));
            };
            let item = item.map_err(|e| async_graphql::Error::new(e.to_string()))?;
            let json = content_item_to_json(&item, &denied);
            Ok(Some(FieldValue::value(json_to_gql(&json))))
        })
    })
    .argument(InputValue::new("id", TypeRef::named(TypeRef::ID)))
    .argument(InputValue::new("slug", TypeRef::named(TypeRef::STRING)));

    // ---- Query: list ----
    let ct_l = ct.name.clone();
    let ctx_l = ctx_tmpl.clone();
    let list = Field::new(&list_name, TypeRef::named_nn_list_nn(type_name.clone()), move |ctx| {
        let ct_l = ct_l.clone();
        let _ctx_l = ctx_l.clone();
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            if !gql_ctx.can(Action::Read, &ct_l) {
                return Err(async_graphql::Error::new("forbidden"));
            }
            let denied = get_denied(gql_ctx, &ct_l);
            let status = ctx.args.try_get("status").ok().and_then(|v| v.string().ok().map(|s| s.to_string()));
            let page = ctx.args.try_get("page").ok().and_then(|v| v.u64().ok()).unwrap_or(1) as u32;
            let per_page = ctx.args.try_get("perPage").ok().and_then(|v| v.u64().ok()).unwrap_or(20) as u32;

            let result = gql_ctx.store.list(&ct_l, ListQuery {
                status,
                pagination: Some(Pagination { page, per_page }),
                ..Default::default()
            }).await.map_err(|e| async_graphql::Error::new(e.to_string()))?;

            let items: Vec<FieldValue> = result.items.iter().map(|item| {
                FieldValue::value(json_to_gql(&content_item_to_json(item, &denied)))
            }).collect();
            Ok(Some(FieldValue::list(items)))
        })
    })
    .argument(InputValue::new("status", TypeRef::named(TypeRef::STRING)))
    .argument(InputValue::new("page", TypeRef::named(TypeRef::INT)))
    .argument(InputValue::new("perPage", TypeRef::named(TypeRef::INT)));

    // ---- Mutation: create ----
    let ct_c = ct.name.clone();
    let create_name = format!("create{type_name}");
    let create = Field::new(&create_name, TypeRef::named_nn(type_name.clone()), move |ctx| {
        let ct_c = ct_c.clone();
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            if !gql_ctx.can(Action::Create, &ct_c) {
                return Err(async_graphql::Error::new("forbidden"));
            }
            let denied = get_denied(gql_ctx, &ct_c);
            let user_id = gql_ctx.user_id.as_deref().unwrap_or("anonymous").to_string();
            let input_val = ctx.args.try_get("input")?.as_value().clone();
            let (fields, status, scheduled_at) = input_to_fields(&input_val);
            let item = gql_ctx.store.create(&ct_c, ContentData { fields, status, scheduled_at }, &user_id)
                .await.map_err(|e| async_graphql::Error::new(e.to_string()))?;
            gql_ctx.cache.invalidate_prefix(&ct_c).await;
            Ok(Some(FieldValue::value(json_to_gql(&content_item_to_json(&item, &denied)))))
        })
    })
    .argument(InputValue::new("input", TypeRef::named_nn(input_name.clone())));

    // ---- Mutation: update ----
    let ct_u = ct.name.clone();
    let update_name = format!("update{type_name}");
    let update = Field::new(&update_name, TypeRef::named_nn(type_name.clone()), move |ctx| {
        let ct_u = ct_u.clone();
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            if !gql_ctx.can(Action::Update, &ct_u) {
                return Err(async_graphql::Error::new("forbidden"));
            }
            let denied = get_denied(gql_ctx, &ct_u);
            let user_id = gql_ctx.user_id.as_deref().unwrap_or("anonymous").to_string();
            let id = ctx.args.try_get("id")?.string()?.to_string();
            let input_val = ctx.args.try_get("input")?.as_value().clone();
            let (fields, status, scheduled_at) = input_to_fields(&input_val);
            let item = gql_ctx.store.update(&ct_u, &id, ContentData { fields, status, scheduled_at }, &user_id)
                .await.map_err(|e| async_graphql::Error::new(e.to_string()))?;
            gql_ctx.cache.invalidate_prefix(&ct_u).await;
            Ok(Some(FieldValue::value(json_to_gql(&content_item_to_json(&item, &denied)))))
        })
    })
    .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)))
    .argument(InputValue::new("input", TypeRef::named_nn(input_name)));

    // ---- Mutation: transition ----
    let ct_t = ct.name.clone();
    let trans_name = format!("transition{type_name}");
    let transition = Field::new(&trans_name, TypeRef::named_nn(type_name.clone()), move |ctx| {
        let ct_t = ct_t.clone();
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            let denied = get_denied(gql_ctx, &ct_t);
            let user_id = gql_ctx.user_id.as_deref().unwrap_or("anonymous").to_string();
            let id = ctx.args.try_get("id")?.string()?.to_string();
            let to_state = ctx.args.try_get("toState")?.string()?.to_string();
            if !gql_ctx.rbac.can_transition(&gql_ctx.user_roles, &ct_t, "", &to_state) {
                return Err(async_graphql::Error::new("transition not allowed for your role"));
            }
            let item = gql_ctx.store.transition(&ct_t, &id, &to_state, &user_id)
                .await.map_err(|e| async_graphql::Error::new(e.to_string()))?;
            gql_ctx.cache.invalidate_prefix(&ct_t).await;
            Ok(Some(FieldValue::value(json_to_gql(&content_item_to_json(&item, &denied)))))
        })
    })
    .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)))
    .argument(InputValue::new("toState", TypeRef::named_nn(TypeRef::STRING)));

    // ---- Mutation: delete ----
    let ct_d = ct.name.clone();
    let del_name = format!("delete{type_name}");
    let delete = Field::new(&del_name, TypeRef::named_nn(TypeRef::BOOLEAN), move |ctx| {
        let ct_d = ct_d.clone();
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            if !gql_ctx.can(Action::Delete, &ct_d) {
                return Err(async_graphql::Error::new("forbidden"));
            }
            let id = ctx.args.try_get("id")?.string()?.to_string();
            gql_ctx.store.delete(&ct_d, &id).await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            gql_ctx.cache.invalidate_prefix(&ct_d).await;
            Ok(Some(FieldValue::value(Value::Boolean(true))))
        })
    })
    .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)));

    let q_fields = vec![single, list];
    let m_fields = vec![create, update, transition, delete];
    (obj, input_obj, q_fields, m_fields)
}

/// Resolver that extracts a named field from the parent JSON Value
fn extract_json_field(key: &'static str) -> impl Fn(ResolverContext) -> FieldFuture + Send + Sync + 'static {
    move |ctx| {
        FieldFuture::new(async move {
            if let Some(Value::Object(obj)) = ctx.parent_value.as_value() {
                return Ok(obj.get(key).cloned().map(FieldValue::value));
            }
            Ok(None)
        })
    }
}

fn extract_json_field_owned(key: String) -> impl Fn(ResolverContext) -> FieldFuture + Send + Sync + 'static {
    move |ctx| {
        let key = key.clone();
        FieldFuture::new(async move {
            if let Some(Value::Object(obj)) = ctx.parent_value.as_value() {
                return Ok(obj.get(key.as_str()).cloned().map(FieldValue::value));
            }
            Ok(None)
        })
    }
}

fn pascal(s: &str) -> String {
    s.split('_').map(|w| {
        let mut c = w.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    }).collect()
}
