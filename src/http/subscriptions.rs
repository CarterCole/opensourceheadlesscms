use async_graphql::{dynamic::Schema, http::ALL_WEBSOCKET_PROTOCOLS, Data};
use async_graphql_axum::{GraphQLProtocol, GraphQLWebSocket};
use axum::{
    extract::{ws::WebSocketUpgrade, Extension, State},
    response::{IntoResponse, Response},
};

use crate::http::auth::authenticate_token;
use crate::schema::SchemaContext;

#[derive(Clone)]
pub struct GraphQLWsState {
    pub schema: Schema,
    pub ctx_template: SchemaContext,
    pub jwt_secret: String,
}

/// Websocket entrypoint for GraphQL subscriptions.
///
/// Browsers authenticate with the HttpOnly session cookie, which `jwt_auth_middleware` has
/// already resolved (including an `Origin` check, which guards against cross-site websocket
/// hijacking) into the request's `SchemaContext`. Non-browser clients can instead send
/// `{"Authorization": "Bearer <jwt>"}` in the `graphql-ws` `connection_init` payload.
/// Unauthenticated sockets are refused.
pub async fn graphql_ws_handler(
    State(state): State<GraphQLWsState>,
    Extension(request_ctx): Extension<SchemaContext>,
    protocol: GraphQLProtocol,
    ws: WebSocketUpgrade,
) -> Response {
    let cookie_ctx = request_ctx.is_authenticated().then_some(request_ctx);

    ws.protocols(ALL_WEBSOCKET_PROTOCOLS)
        .on_upgrade(move |socket| {
            GraphQLWebSocket::new(socket, state.schema, protocol)
                .on_connection_init(move |payload| {
                    let cookie_ctx = cookie_ctx.clone();
                    let ctx_template = state.ctx_template.clone();
                    let jwt_secret = state.jwt_secret.clone();
                    async move {
                        let ctx = match cookie_ctx {
                            Some(ctx) => ctx,
                            None => {
                                let token = payload
                                    .get("Authorization")
                                    .or_else(|| payload.get("authorization"))
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| s.strip_prefix("Bearer "));
                                let authed = match token {
                                    Some(t) => {
                                        authenticate_token(
                                            ctx_template.store.as_ref(),
                                            &jwt_secret,
                                            t,
                                        )
                                        .await
                                    }
                                    None => None,
                                };
                                let (user_id, roles) = authed.ok_or_else(|| {
                                    async_graphql::Error::new("authentication required")
                                })?;
                                ctx_template.with_user(user_id, roles)
                            }
                        };
                        let mut data = Data::default();
                        data.insert(ctx);
                        Ok(data)
                    }
                })
                .serve()
        })
        .into_response()
}
