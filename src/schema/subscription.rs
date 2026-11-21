use async_graphql::{dynamic::*, Name, Value};
use async_stream::stream;
use tokio::sync::broadcast::error::RecvError;

use crate::core::{Action, ContentEvent, MutationType};
use crate::schema::context::SchemaContext;
use crate::schema::resolvers::{json_to_gql, redact_item_json};

/// Returns the `MutationType` enum, `ContentEvent` object, and `Subscription` root
/// as separate dynamic types for the caller to `.register(...)` on the schema.
pub fn build_subscription() -> (Enum, Object, Subscription) {
    let mutation_type_enum = Enum::new("MutationType")
        .item("CREATED")
        .item("UPDATED")
        .item("DELETED")
        .item("TRANSITIONED");

    let content_event = Object::new("ContentEvent")
        .field(Field::new(
            "contentType",
            TypeRef::named_nn(TypeRef::STRING),
            |ctx| {
                FieldFuture::new(async move {
                    let ev = ctx.parent_value.try_downcast_ref::<ContentEvent>()?;
                    Ok(Some(FieldValue::value(Value::String(
                        ev.content_type.clone(),
                    ))))
                })
            },
        ))
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move {
                let ev = ctx.parent_value.try_downcast_ref::<ContentEvent>()?;
                Ok(Some(FieldValue::value(Value::String(ev.id.clone()))))
            })
        }))
        .field(Field::new(
            "mutationType",
            TypeRef::named_nn("MutationType"),
            |ctx| {
                FieldFuture::new(async move {
                    let ev = ctx.parent_value.try_downcast_ref::<ContentEvent>()?;
                    let name = match ev.mutation_type {
                        MutationType::Created => "CREATED",
                        MutationType::Updated => "UPDATED",
                        MutationType::Deleted => "DELETED",
                        MutationType::Transitioned => "TRANSITIONED",
                    };
                    Ok(Some(FieldValue::value(Value::Enum(Name::new(name)))))
                })
            },
        ))
        .field(Field::new("item", TypeRef::named("ContentItem"), |ctx| {
            FieldFuture::new(async move {
                let ev = ctx.parent_value.try_downcast_ref::<ContentEvent>()?;
                Ok(ev
                    .item
                    .as_ref()
                    .map(|json| FieldValue::value(json_to_gql(json))))
            })
        }));

    let subscription = Subscription::new("Subscription").field(
        SubscriptionField::new("contentChanged", TypeRef::named_nn("ContentEvent"), |ctx| {
            SubscriptionFieldFuture::new(async move {
                let gql_ctx = ctx.ctx.data::<SchemaContext>()?.clone();
                let type_filter = ctx.args.try_get("type")?.string()?.to_string();
                let id_filter = ctx
                    .args
                    .try_get("id")
                    .ok()
                    .and_then(|v| v.string().ok().map(|s| s.to_string()));

                if !gql_ctx.can(Action::Read, &type_filter) {
                    return Err(async_graphql::Error::new("forbidden"));
                }

                // Events are built with the *mutating* user's view; redact per subscriber.
                let denied = match gql_ctx.rbac.allowed_fields(
                    &gql_ctx.user_roles,
                    &type_filter,
                    Action::Read,
                ) {
                    crate::core::AllowedFields::Except(d) => d,
                    _ => vec![],
                };
                let mut rx = gql_ctx.events.subscribe();
                Ok(stream! {
                    loop {
                        match rx.recv().await {
                            Ok(mut ev) => {
                                if ev.content_type != type_filter {
                                    continue;
                                }
                                if let Some(id) = &id_filter {
                                    if *id != ev.id {
                                        continue;
                                    }
                                }
                                if let Some(item) = ev.item.as_mut() {
                                    redact_item_json(item, &denied);
                                }
                                yield Ok(FieldValue::owned_any(ev));
                            }
                            Err(RecvError::Lagged(_)) => continue,
                            Err(RecvError::Closed) => break,
                        }
                    }
                })
            })
        })
        .argument(InputValue::new("type", TypeRef::named_nn(TypeRef::STRING)))
        .argument(InputValue::new("id", TypeRef::named(TypeRef::ID))),
    );

    (mutation_type_enum, content_event, subscription)
}
