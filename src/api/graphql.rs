use async_graphql::http::{GraphQLPlaygroundConfig, playground_source};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::response::{Html, IntoResponse};
use axum::{extract::State, http::StatusCode};
use seaography::{Builder, BuilderContext, lazy_static::lazy_static};

use crate::{
    app::state::AppState,
    database::{register_active_enums, register_entity_modules},
};

lazy_static! {
    static ref CONTEXT: BuilderContext = BuilderContext::default();
}

pub async fn graphql_playground(State(state): State<AppState>) -> impl IntoResponse {
    if !state.config().get().graphql.playground {
        return StatusCode::FORBIDDEN.into_response();
    }
    // Setup GraphQL playground web and specify the endpoint for GraphQL resolver
    let endpoint = state.config().get().graphql.endpoint.clone();
    let config = GraphQLPlaygroundConfig::new(&endpoint); //.with_header("Authorization", "");

    Html(playground_source(config)).into_response()
}

pub async fn graphql_handler(
    State(state): State<AppState>,
    //_headers: HeaderMap,
    req: GraphQLRequest,
) -> Result<GraphQLResponse, (StatusCode, &'static str)> {
    //check_user_auth(&headers)?;
    let mut builder = Builder::new(&CONTEXT, state.database());
    builder = register_entity_modules(builder);
    builder = register_active_enums(builder);
    let builder = builder
        .set_depth_limit(state.config().get().graphql.depth)
        .set_complexity_limit(state.config().get().graphql.depth)
        .schema_builder()
        .data(state.database());

    let schema = builder.finish().expect("Failed to build GraphQL schema");

    Ok(schema.execute(req.into_inner()).await.into())
}
