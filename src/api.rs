use axum::{Router, routing::get, routing::post};
use color_eyre::Result;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::{api, app::state::AppState};

pub mod graphql;

pub fn serve(state: AppState) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let router = Router::new()
            .route(
                &state.config().get().graphql.endpoint,
                post(api::graphql::graphql_handler),
            )
            .route(
                &state.config().get().graphql.endpoint,
                get(api::graphql::graphql_playground),
            )
            .with_state(state.clone())
            .layer(TraceLayer::new_for_http());

        let listener = tokio::net::TcpListener::bind(&state.config().get().api.bind)
            .await
            .unwrap();

        info!("API Listening on {}", listener.local_addr()?);

        let result = axum::serve(listener, router)
            .with_graceful_shutdown(state.api_shutdown.clone().notified_owned())
            .await;

        state.api_shutdown_complete.notify_waiters();

        Ok(result?)
    })
}
