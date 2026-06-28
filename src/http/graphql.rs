use async_graphql::{dynamic::Schema, http::GraphQLPlaygroundConfig, Request as GqlRequest};
use axum::{
    body::Body,
    extract::{Extension, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use crate::schema::SchemaContext;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct GqlRequestBody {
    pub query: String,
    #[serde(default)]
    pub variables: Option<serde_json::Value>,
    #[serde(rename = "operationName")]
    pub operation_name: Option<String>,
}

pub async fn graphql_handler(
    State(schema): State<Schema>,
    Extension(ctx): Extension<SchemaContext>,
    Json(body): Json<GqlRequestBody>,
) -> Response {
    let mut req = GqlRequest::new(body.query);
    if let Some(vars) = body.variables {
        let vars_map = async_graphql::Variables::from_json(vars);
        req = req.variables(vars_map);
    }
    if let Some(op) = body.operation_name {
        req = req.operation_name(op);
    }
    req = req.data(ctx);

    let resp = schema.execute(req).await;
    let body = serde_json::to_string(&resp).unwrap_or_default();

    let status = if resp.errors.is_empty() {
        StatusCode::OK
    } else {
        StatusCode::OK // GraphQL errors still return 200
    };

    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub async fn graphql_playground_handler() -> impl IntoResponse {
    Html(async_graphql::http::playground_source(
        GraphQLPlaygroundConfig::new("/graphql"),
    ))
}
