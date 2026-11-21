use crate::config::{CmsConfig, ContentTypeConfig, FieldType};
use crate::core::{
    Action, AllowedFields, ContentData, FieldValue as CmsFieldValue, ListQuery, Pagination,
};
use async_graphql::{dynamic::*, Value};
use std::collections::HashMap;

use crate::schema::context::SchemaContext;
use crate::schema::resolvers::{content_item_to_json, input_to_fields, json_to_gql};

pub fn build_schema(
    config: &CmsConfig,
    ctx_template: SchemaContext,
) -> Result<Schema, SchemaError> {
    let content_item = Object::new("ContentItem")
        .field(Field::new(
            "id",
            TypeRef::named_nn(TypeRef::ID),
            extract_json_field("id"),
        ))
        .field(Field::new(
            "contentType",
            TypeRef::named_nn(TypeRef::STRING),
            extract_json_field("contentType"),
        ))
        .field(Field::new(
            "status",
            TypeRef::named_nn(TypeRef::STRING),
            extract_json_field("status"),
        ))
        .field(Field::new(
            "version",
            TypeRef::named_nn(TypeRef::INT),
            extract_json_field("version"),
        ))
        .field(Field::new(
            "createdAt",
            TypeRef::named_nn("DateTime"),
            extract_json_field("createdAt"),
        ))
        .field(Field::new(
            "updatedAt",
            TypeRef::named_nn("DateTime"),
            extract_json_field("updatedAt"),
        ))
        .field(Field::new(
            "createdBy",
            TypeRef::named_nn(TypeRef::STRING),
            extract_json_field("createdBy"),
        ))
        .field(Field::new(
            "updatedBy",
            TypeRef::named_nn(TypeRef::STRING),
            extract_json_field("updatedBy"),
        ))
        .field(Field::new(
            "fields",
            TypeRef::named_nn("JSON"),
            extract_json_field("fields"),
        ));

    // ---- Query: content(type, id, slug) ----
    let ctx_single = ctx_template.clone();
    let content_field = Field::new("content", TypeRef::named("ContentItem"), move |ctx| {
        let _ctx_single = ctx_single.clone();
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            let ct = ctx.args.try_get("type")?.string()?.to_string();
            if !gql_ctx.can(Action::Read, &ct) {
                return Err(async_graphql::Error::new("forbidden"));
            }
            let denied = get_denied(gql_ctx, &ct);
            let item = if let Ok(acc) = ctx.args.try_get("id") {
                gql_ctx.store.get(&ct, acc.string()?).await
            } else if let Ok(acc) = ctx.args.try_get("slug") {
                gql_ctx.store.get_by_slug(&ct, acc.string()?).await
            } else {
                return Err(async_graphql::Error::new("provide id or slug"));
            };
            let item = item.map_err(|e| async_graphql::Error::new(e.to_string()))?;
            let json = content_item_to_json(&item, &denied);
            Ok(Some(FieldValue::value(json_to_gql(&json))))
        })
    })
    .argument(InputValue::new("type", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("id", TypeRef::named(TypeRef::ID)))
    .argument(InputValue::new("slug", TypeRef::named(TypeRef::STRING)));

    // ---- Query: contentList(type, status, page, perPage) ----
    let ctx_list = ctx_template.clone();
    let content_list_field = Field::new(
        "contentList",
        TypeRef::named_nn_list_nn("ContentItem"),
        move |ctx| {
            let _ctx_list = ctx_list.clone();
            FieldFuture::new(async move {
                let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
                let ct = ctx.args.try_get("type")?.string()?.to_string();
                if !gql_ctx.can(Action::Read, &ct) {
                    return Err(async_graphql::Error::new("forbidden"));
                }
                let denied = get_denied(gql_ctx, &ct);
                let status = ctx
                    .args
                    .try_get("status")
                    .ok()
                    .and_then(|v| v.string().ok().map(|s| s.to_string()));
                let page = ctx
                    .args
                    .try_get("page")
                    .ok()
                    .and_then(|v| v.u64().ok())
                    .unwrap_or(1)
                    .clamp(1, u32::MAX as u64) as u32;
                // Cap page size so a single query can't pull the whole table.
                let per_page = ctx
                    .args
                    .try_get("perPage")
                    .ok()
                    .and_then(|v| v.u64().ok())
                    .unwrap_or(20)
                    .clamp(1, MAX_PER_PAGE) as u32;

                let result = gql_ctx
                    .store
                    .list(
                        &ct,
                        ListQuery {
                            status,
                            pagination: Some(Pagination { page, per_page }),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;

                let items: Vec<FieldValue> = result
                    .items
                    .iter()
                    .map(|item| {
                        FieldValue::value(json_to_gql(&content_item_to_json(item, &denied)))
                    })
                    .collect();
                Ok(Some(FieldValue::list(items)))
            })
        },
    )
    .argument(InputValue::new("type", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("status", TypeRef::named(TypeRef::STRING)))
    .argument(InputValue::new("page", TypeRef::named(TypeRef::INT)))
    .argument(InputValue::new("perPage", TypeRef::named(TypeRef::INT)));

    // ---- Query: search ----
    let ctx_s = ctx_template.clone();
    let search_field = Field::new(
        "search",
        TypeRef::named_nn_list_nn("SearchResult"),
        move |ctx| {
            let ctx_s = ctx_s.clone();
            FieldFuture::new(async move {
                let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
                let _ctx_s = ctx_s.clone();
                let q = ctx.args.try_get("query")?.string()?.to_string();
                let ct = ctx
                    .args
                    .try_get("contentType")
                    .ok()
                    .and_then(|v| v.string().ok().map(|s| s.to_string()));
                let page = ctx
                    .args
                    .try_get("page")
                    .ok()
                    .and_then(|v| v.u64().ok())
                    .unwrap_or(1) as u32;

                if q.len() > 256 {
                    return Err(async_graphql::Error::new("query too long"));
                }
                let result = gql_ctx
                    .store
                    .search(
                        ct.as_deref(),
                        &q,
                        Pagination {
                            page: page.max(1),
                            per_page: 20,
                        },
                    )
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;

                // The search index spans every type and status: only return items
                // the caller is actually allowed to read.
                let items: Vec<FieldValue> = result
                    .items
                    .iter()
                    .filter(|item| gql_ctx.can(Action::Read, &item.content_type))
                    .map(|item| {
                        let denied = get_denied(gql_ctx, &item.content_type);
                        let json = content_item_to_json(item, &denied);
                        FieldValue::value(json_to_gql(&json))
                    })
                    .collect();
                Ok(Some(FieldValue::list(items)))
            })
        },
    )
    .argument(InputValue::new("query", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new(
        "contentType",
        TypeRef::named(TypeRef::STRING),
    ))
    .argument(InputValue::new("page", TypeRef::named(TypeRef::INT)));

    let search_result = Object::new("SearchResult")
        .field(Field::new(
            "id",
            TypeRef::named_nn(TypeRef::ID),
            extract_json_field("id"),
        ))
        .field(Field::new(
            "contentType",
            TypeRef::named_nn(TypeRef::STRING),
            extract_json_field("contentType"),
        ))
        .field(Field::new(
            "status",
            TypeRef::named_nn(TypeRef::STRING),
            extract_json_field("status"),
        ));

    // ---- Globals: type Global, query globals / global(slug|name), mutation setGlobal ----
    let global_type = ["slug", "name", "label", "kind", "item", "storage"]
        .into_iter()
        .fold(Object::new("Global"), |o, f| {
            o.field(Field::new(
                f,
                TypeRef::named_nn(TypeRef::STRING),
                extract_json_field(f),
            ))
        })
        .field(Field::new(
            "description",
            TypeRef::named(TypeRef::STRING),
            extract_json_field("description"),
        ))
        .field(Field::new(
            "public",
            TypeRef::named_nn(TypeRef::BOOLEAN),
            extract_json_field("public"),
        ))
        .field(Field::new(
            "value",
            TypeRef::named("JSON"),
            extract_json_field("value"),
        ))
        .field(Field::new(
            "updatedAt",
            TypeRef::named("DateTime"),
            extract_json_field("updatedAt"),
        ))
        .field(Field::new(
            "updatedBy",
            TypeRef::named(TypeRef::STRING),
            extract_json_field("updatedBy"),
        ));

    let globals_field = Field::new("globals", TypeRef::named_nn_list_nn("Global"), |ctx| {
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            let list_arg = |k: &str| -> Option<Vec<String>> {
                let v = ctx.args.get(k)?;
                let list = v.list().ok()?;
                Some(
                    list.iter()
                        .filter_map(|x| x.string().ok().map(str::to_string))
                        .collect(),
                )
            };
            let (slugs, names) = (list_arg("slugs"), list_arg("names"));
            let mut defs: Vec<_> = gql_ctx
                .config()
                .globals
                .values()
                .filter(|d| gql_ctx.can_read_global(d))
                .filter(|d| {
                    (slugs.is_none() && names.is_none())
                        || slugs.as_ref().is_some_and(|s| s.contains(&d.slug()))
                        || names.as_ref().is_some_and(|n| n.contains(&d.name))
                })
                .collect();
            defs.sort_by_key(|d| d.slug());
            let mut out = Vec::with_capacity(defs.len());
            for def in defs {
                let r = gql_ctx.globals.read(def).await.map_err(gql_err)?;
                out.push(FieldValue::value(global_to_gql(gql_ctx, &r)));
            }
            Ok(Some(FieldValue::list(out)))
        })
    })
    .argument(InputValue::new(
        "slugs",
        TypeRef::named_list(TypeRef::STRING),
    ))
    .argument(InputValue::new(
        "names",
        TypeRef::named_list(TypeRef::STRING),
    ));

    // ---- Query: site { <camelSlug> … } — one typed field per global ----
    // Fields resolve independently, so a query reads only the globals it names.
    let mut site_defs: Vec<_> = config.globals.values().collect();
    site_defs.sort_by_key(|d| d.slug());
    let site_type = site_defs.iter().fold(Object::new("Site"), |obj, def| {
        let name = def.name.clone();
        let ty = match (def.kind, def.item) {
            (_, crate::config::GlobalItem::Json) | (crate::config::GlobalKind::Map, _) => {
                TypeRef::named("JSON")
            }
            (crate::config::GlobalKind::Single, _) => TypeRef::named(TypeRef::STRING),
            (crate::config::GlobalKind::List | crate::config::GlobalKind::Set, _) => {
                TypeRef::named_nn_list(TypeRef::STRING)
            }
        };
        let mut field = Field::new(def.js_ident(), ty, move |ctx| {
            let name = name.clone();
            FieldFuture::new(async move {
                let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
                // Unreadable (non-public, anonymous) → null, as with `global`.
                let Some(def) = gql_ctx
                    .config()
                    .global(&name)
                    .filter(|d| gql_ctx.can_read_global(d))
                else {
                    return Ok(None);
                };
                let r = gql_ctx.globals.read(def).await.map_err(gql_err)?;
                Ok(Some(FieldValue::value(json_to_gql(&r.value))))
            })
        });
        let desc = def
            .description
            .clone()
            .unwrap_or_else(|| def.display_label().to_string());
        field = field.description(format!("{desc} (`{}`)", def.slug()));
        obj.field(field)
    });
    let site_field = (!site_defs.is_empty()).then(|| {
        Field::new("site", TypeRef::named_nn("Site"), |_| {
            FieldFuture::new(async { Ok(Some(FieldValue::owned_any(()))) })
        })
        .description("Site-wide globals, one field per global (camelCase of its slug)")
    });

    let global_field = Field::new("global", TypeRef::named("Global"), |ctx| {
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            let Some(def) = find_global(gql_ctx, &ctx)?.filter(|d| gql_ctx.can_read_global(d))
            else {
                return Ok(None);
            };
            let r = gql_ctx.globals.read(def).await.map_err(gql_err)?;
            Ok(Some(FieldValue::value(global_to_gql(gql_ctx, &r))))
        })
    })
    .argument(InputValue::new("slug", TypeRef::named(TypeRef::STRING)))
    .argument(InputValue::new("name", TypeRef::named(TypeRef::STRING)));

    let set_global_field = Field::new("setGlobal", TypeRef::named_nn("Global"), |ctx| {
        FieldFuture::new(async move {
            let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
            let Some(user_id) = gql_ctx.user_id.clone() else {
                return Err(async_graphql::Error::new("authentication required"));
            };
            if !gql_ctx.can_write_globals() {
                return Err(async_graphql::Error::new("forbidden"));
            }
            let def = find_global(gql_ctx, &ctx)?
                .ok_or_else(|| async_graphql::Error::new("unknown global"))?;
            let value = ctx
                .args
                .try_get("value")?
                .as_value()
                .clone()
                .into_json()
                .map_err(gql_err)?;
            let r = gql_ctx
                .globals
                .write(def, value, &user_id)
                .await
                .map_err(gql_err)?;
            gql_ctx.cache.invalidate_prefix("_globals:").await;
            Ok(Some(FieldValue::value(global_to_gql(gql_ctx, &r))))
        })
    })
    .argument(InputValue::new("slug", TypeRef::named(TypeRef::STRING)))
    .argument(InputValue::new("name", TypeRef::named(TypeRef::STRING)))
    .argument(InputValue::new("value", TypeRef::named_nn("JSON")));

    let query = Object::new("Query")
        .field(content_field)
        .field(content_list_field)
        .field(search_field)
        .field(globals_field)
        .field(global_field);
    let query = match site_field {
        Some(f) => query.field(f),
        None => query,
    };

    // ---- Mutation: createContent ----
    let ctx_c = ctx_template.clone();
    let create_field = Field::new(
        "createContent",
        TypeRef::named_nn("ContentItem"),
        move |ctx| {
            let _ctx_c = ctx_c.clone();
            FieldFuture::new(async move {
                let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
                let ct = ctx.args.try_get("type")?.string()?.to_string();
                if !gql_ctx.can(Action::Create, &ct) {
                    return Err(async_graphql::Error::new("forbidden"));
                }
                let denied = get_denied(gql_ctx, &ct);
                let user_id = gql_ctx
                    .user_id
                    .as_deref()
                    .unwrap_or("anonymous")
                    .to_string();
                let input_val = ctx.args.try_get("input")?.as_value().clone();
                let (fields, mut status, scheduled_at) = input_to_fields(&input_val);
                enforce_write_policy(gql_ctx, &ct, Action::Create, &fields, &mut status, None)?;
                let item = gql_ctx
                    .store
                    .create(
                        &ct,
                        ContentData {
                            fields,
                            status,
                            scheduled_at,
                        },
                        &user_id,
                    )
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                gql_ctx.cache.invalidate_prefix(&ct).await;
                let json = content_item_to_json(&item, &denied);
                gql_ctx.events.publish(crate::core::ContentEvent {
                    content_type: ct.clone(),
                    id: item.id.clone(),
                    mutation_type: crate::core::MutationType::Created,
                    item: Some(json.clone()),
                });
                Ok(Some(FieldValue::value(json_to_gql(&json))))
            })
        },
    )
    .argument(InputValue::new("type", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("input", TypeRef::named_nn("JSON")));

    // ---- Mutation: updateContent ----
    let ctx_u = ctx_template.clone();
    let update_field = Field::new(
        "updateContent",
        TypeRef::named_nn("ContentItem"),
        move |ctx| {
            let _ctx_u = ctx_u.clone();
            FieldFuture::new(async move {
                let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
                let ct = ctx.args.try_get("type")?.string()?.to_string();
                if !gql_ctx.can(Action::Update, &ct) {
                    return Err(async_graphql::Error::new("forbidden"));
                }
                let denied = get_denied(gql_ctx, &ct);
                let user_id = gql_ctx
                    .user_id
                    .as_deref()
                    .unwrap_or("anonymous")
                    .to_string();
                let id = ctx.args.try_get("id")?.string()?.to_string();
                let input_val = ctx.args.try_get("input")?.as_value().clone();
                let (mut fields, mut status, scheduled_at) = input_to_fields(&input_val);
                let existing = gql_ctx
                    .store
                    .get(&ct, &id)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                enforce_write_policy(
                    gql_ctx,
                    &ct,
                    Action::Update,
                    &fields,
                    &mut status,
                    Some(&existing.fields),
                )?;
                preserve_restricted_fields(gql_ctx, &ct, &existing.fields, &mut fields);
                let item = gql_ctx
                    .store
                    .update(
                        &ct,
                        &id,
                        ContentData {
                            fields,
                            status,
                            scheduled_at,
                        },
                        &user_id,
                    )
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                gql_ctx.cache.invalidate_prefix(&ct).await;
                let json = content_item_to_json(&item, &denied);
                gql_ctx.events.publish(crate::core::ContentEvent {
                    content_type: ct.clone(),
                    id: item.id.clone(),
                    mutation_type: crate::core::MutationType::Updated,
                    item: Some(json.clone()),
                });
                Ok(Some(FieldValue::value(json_to_gql(&json))))
            })
        },
    )
    .argument(InputValue::new("type", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)))
    .argument(InputValue::new("input", TypeRef::named_nn("JSON")));

    // ---- Mutation: transitionContent ----
    let ctx_t = ctx_template.clone();
    let transition_field = Field::new(
        "transitionContent",
        TypeRef::named_nn("ContentItem"),
        move |ctx| {
            let _ctx_t = ctx_t.clone();
            FieldFuture::new(async move {
                let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
                let ct = ctx.args.try_get("type")?.string()?.to_string();
                // Workflow rules may list no roles ("anyone"), which must still mean
                // "any signed-in user", never an anonymous caller.
                let Some(user_id) = gql_ctx.user_id.clone() else {
                    return Err(async_graphql::Error::new("authentication required"));
                };
                if !gql_ctx.can(Action::Read, &ct) {
                    return Err(async_graphql::Error::new("forbidden"));
                }
                let denied = get_denied(gql_ctx, &ct);
                let id = ctx.args.try_get("id")?.string()?.to_string();
                let to_state = ctx.args.try_get("toState")?.string()?.to_string();
                // Validate against the item's real current state, not an empty one.
                let current = gql_ctx
                    .store
                    .get(&ct, &id)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                if !gql_ctx.rbac.can_transition(
                    &gql_ctx.user_roles,
                    &ct,
                    &current.status,
                    &to_state,
                ) {
                    return Err(async_graphql::Error::new(
                        "transition not allowed for your role",
                    ));
                }
                let item = gql_ctx
                    .store
                    .transition(&ct, &id, &to_state, &user_id)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                gql_ctx.cache.invalidate_prefix(&ct).await;
                let json = content_item_to_json(&item, &denied);
                gql_ctx.events.publish(crate::core::ContentEvent {
                    content_type: ct.clone(),
                    id: item.id.clone(),
                    mutation_type: crate::core::MutationType::Transitioned,
                    item: Some(json.clone()),
                });
                Ok(Some(FieldValue::value(json_to_gql(&json))))
            })
        },
    )
    .argument(InputValue::new("type", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)))
    .argument(InputValue::new(
        "toState",
        TypeRef::named_nn(TypeRef::STRING),
    ));

    // ---- Mutation: deleteContent ----
    let ctx_d = ctx_template.clone();
    let delete_field = Field::new(
        "deleteContent",
        TypeRef::named_nn(TypeRef::BOOLEAN),
        move |ctx| {
            let _ctx_d = ctx_d.clone();
            FieldFuture::new(async move {
                let gql_ctx = ctx.ctx.data::<SchemaContext>()?;
                let ct = ctx.args.try_get("type")?.string()?.to_string();
                if !gql_ctx.can(Action::Delete, &ct) {
                    return Err(async_graphql::Error::new("forbidden"));
                }
                let id = ctx.args.try_get("id")?.string()?.to_string();
                gql_ctx
                    .store
                    .delete(&ct, &id)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                gql_ctx.cache.invalidate_prefix(&ct).await;
                gql_ctx.events.publish(crate::core::ContentEvent {
                    content_type: ct.clone(),
                    id: id.clone(),
                    mutation_type: crate::core::MutationType::Deleted,
                    item: None,
                });
                Ok(Some(FieldValue::value(Value::Boolean(true))))
            })
        },
    )
    .argument(InputValue::new("type", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)));

    let mutation = Object::new("Mutation")
        .field(create_field)
        .field(update_field)
        .field(transition_field)
        .field(delete_field)
        .field(set_global_field);

    let (mutation_type_enum, content_event, subscription) =
        crate::schema::subscription::build_subscription();

    let schema = Schema::build("Query", Some("Mutation"), Some("Subscription"))
        .register(Scalar::new("JSON"))
        .register(Scalar::new("DateTime"))
        .register(content_item)
        .register(search_result)
        .register(global_type);
    // An object type with no fields is invalid GraphQL, so `Site` only exists with globals.
    let schema = if site_defs.is_empty() {
        schema
    } else {
        schema.register(site_type)
    };
    let schema = schema
        .register(query)
        .register(mutation)
        .register(mutation_type_enum)
        .register(content_event)
        .register(subscription)
        // Bound query nesting; 20 leaves ample room for introspection (~14 levels deep).
        .limit_depth(20)
        .data(ctx_template)
        .finish()?;

    Ok(schema)
}

fn gql_err(e: impl std::fmt::Display) -> async_graphql::Error {
    async_graphql::Error::new(e.to_string())
}

/// Resolve a `slug:` or `name:` argument (exactly one) to its definition.
fn find_global<'a>(
    gql_ctx: &'a SchemaContext,
    ctx: &ResolverContext,
) -> Result<Option<&'a crate::config::GlobalConfig>, async_graphql::Error> {
    let arg = |k: &str| {
        ctx.args
            .get(k)
            .and_then(|v| v.string().ok().map(str::to_string))
    };
    match (arg("slug"), arg("name")) {
        (Some(slug), None) => Ok(gql_ctx.config().global_by_slug(&slug)),
        (None, Some(name)) => Ok(gql_ctx.config().global(&name)),
        _ => Err(async_graphql::Error::new(
            "provide exactly one of slug or name",
        )),
    }
}

fn global_to_gql(ctx: &SchemaContext, r: &crate::globals::ResolvedGlobal) -> Value {
    let lower = |v: serde_json::Value| v.as_str().unwrap_or_default().to_string();
    json_to_gql(&serde_json::json!({
        "slug": r.def.slug(),
        "name": r.def.name,
        "label": r.def.display_label(),
        "description": r.def.description,
        "kind": lower(serde_json::json!(r.def.kind)),
        "item": lower(serde_json::json!(r.def.item)),
        "storage": lower(serde_json::json!(r.def.storage.unwrap_or(ctx.config().server.globals.storage))),
        "public": r.def.public,
        "value": r.value,
        "updatedAt": r.stored.as_ref().and_then(|s| s.updated_at).map(|t| t.to_rfc3339()),
        "updatedBy": r.stored.as_ref().and_then(|s| s.updated_by.clone()),
    }))
}

/// Largest page a GraphQL list query may request.
const MAX_PER_PAGE: u64 = 100;

/// Server-side write policy for create/update, which the resolvers previously skipped:
///  * the content type must be defined in config;
///  * the caller may not write fields their role marks `write: false`;
///  * on workflow-governed types `status` can't be set directly (that would bypass the
///    transition rules) — it is dropped and must go through `transitionContent`.
fn enforce_write_policy(
    ctx: &SchemaContext,
    content_type: &str,
    action: Action,
    fields: &HashMap<String, CmsFieldValue>,
    status: &mut Option<String>,
    existing: Option<&HashMap<String, CmsFieldValue>>,
) -> Result<(), async_graphql::Error> {
    let Some(ct_cfg) = ctx.config().content_type(content_type) else {
        return Err(async_graphql::Error::new("unknown content type"));
    };
    if let AllowedFields::Except(denied) =
        ctx.rbac
            .allowed_fields(&ctx.user_roles, content_type, action)
    {
        if let Some(bad) = fields.keys().find(|k| denied.contains(k)) {
            return Err(async_graphql::Error::new(format!(
                "forbidden: cannot write field `{bad}`"
            )));
        }
    }
    if ct_cfg.workflow.is_some() {
        *status = None;
    }
    validate_fields(ct_cfg, fields, existing)
}

/// Largest single text value accepted for a field.
const MAX_TEXT_BYTES: usize = 1_000_000;

/// Check submitted values against the content type definition: known fields, the right
/// value shape for each field type, no empty required fields, and all required fields
/// present on create (`existing == None`). Fields already stored on the item are tolerated
/// even if the definition has since dropped them, so old content stays editable.
fn validate_fields(
    ct: &ContentTypeConfig,
    fields: &HashMap<String, CmsFieldValue>,
    existing: Option<&HashMap<String, CmsFieldValue>>,
) -> Result<(), async_graphql::Error> {
    let err = |m: String| Err(async_graphql::Error::new(m));
    for (name, value) in fields {
        let Some(def) = ct.fields.iter().find(|f| &f.name == name) else {
            if existing.is_some_and(|e| e.contains_key(name)) {
                continue;
            }
            return err(format!("unknown field `{name}`"));
        };
        if matches!(value, CmsFieldValue::Null) {
            if def.required {
                return err(format!("field `{name}` is required"));
            }
            continue;
        }
        let shape_ok = if def.list {
            matches!(value, CmsFieldValue::List(_))
        } else {
            match def.field_type {
                FieldType::Text
                | FieldType::RichText
                | FieldType::Slug
                | FieldType::Image
                | FieldType::Date => matches!(value, CmsFieldValue::Text(_)),
                FieldType::Number => matches!(value, CmsFieldValue::Number(_)),
                FieldType::Bool => matches!(value, CmsFieldValue::Bool(_)),
                FieldType::Relation => {
                    matches!(value, CmsFieldValue::List(_) | CmsFieldValue::Text(_))
                }
                FieldType::List => matches!(value, CmsFieldValue::List(_)),
            }
        };
        if !shape_ok {
            return err(format!("field `{name}` has the wrong type"));
        }
        if let CmsFieldValue::Text(s) = value {
            if def.required && s.trim().is_empty() {
                return err(format!("field `{name}` is required"));
            }
            if s.len() > MAX_TEXT_BYTES {
                return err(format!("field `{name}` is too large"));
            }
        }
    }
    if existing.is_none() {
        for def in ct.fields.iter().filter(|f| f.required) {
            if matches!(fields.get(&def.name), None | Some(CmsFieldValue::Null)) {
                return err(format!("field `{}` is required", def.name));
            }
        }
    }
    Ok(())
}

/// `updateContent` replaces the whole field set, and clients only ever send back the fields
/// they were allowed to read. Carry over stored values for fields the caller may not read or
/// write so an edit by a restricted role can't silently erase them.
fn preserve_restricted_fields(
    ctx: &SchemaContext,
    content_type: &str,
    existing: &HashMap<String, CmsFieldValue>,
    fields: &mut HashMap<String, CmsFieldValue>,
) {
    let mut hidden: Vec<String> = vec![];
    for action in [Action::Read, Action::Update] {
        if let AllowedFields::Except(denied) =
            ctx.rbac
                .allowed_fields(&ctx.user_roles, content_type, action)
        {
            hidden.extend(denied);
        }
    }
    for name in hidden {
        if let Some(value) = existing.get(&name) {
            fields.insert(name, value.clone());
        }
    }
}

fn get_denied(ctx: &SchemaContext, content_type: &str) -> Vec<String> {
    match ctx
        .rbac
        .allowed_fields(&ctx.user_roles, content_type, Action::Read)
    {
        crate::core::AllowedFields::Except(denied) => denied,
        _ => vec![],
    }
}

/// Resolver that extracts a named field from the parent JSON Value
fn extract_json_field(
    key: &'static str,
) -> impl Fn(ResolverContext) -> FieldFuture + Send + Sync + 'static {
    move |ctx| {
        FieldFuture::new(async move {
            if let Some(Value::Object(obj)) = ctx.parent_value.as_value() {
                return Ok(obj.get(key).cloned().map(FieldValue::value));
            }
            Ok(None)
        })
    }
}
