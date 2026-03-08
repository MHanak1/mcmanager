use std::{process::exit, sync::Arc};

use arc_swap::ArcSwap;
use axum::{Router, routing::get, routing::post};
use color_eyre::eyre::Result;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::json;
use tokio::{sync::Notify, task::JoinHandle};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

use crate::{api, app::config::Config, database};

#[derive(Clone)]
pub struct AppState {
    database: Arc<ArcSwap<DatabaseConnection>>,
    config: Config,
    api_shutdown: Arc<tokio::sync::Notify>,
    api_shutdown_complete: Arc<tokio::sync::Notify>,
}

impl AppState {
    pub async fn new() -> Result<Self> {
        let config = Config::new()?;
        let dbconfig = &config.get().database.clone();
        let database = Arc::new(ArcSwap::from_pointee(
            crate::database::create_database(dbconfig).await?,
        ));
        let api_shutdown = Arc::new(Notify::new());
        let api_shutdown_complete = Arc::new(Notify::new());

        let state = Self {
            config,
            database,
            api_shutdown,
            api_shutdown_complete,
        };
        state.watch_for_config_changes();
        state.spawn_api();

        if !database::Data::find()
            .filter(database::data::Column::Key.eq("completed_setup"))
            .one(&state.database())
            .await?
            .map(|value| value.value == json!(true))
            .unwrap_or(false)
        {
            database::first_launch(&state).await?;
        }

        Ok(state)
    }

    pub fn database(&self) -> DatabaseConnection {
        (*self.database.load_full()).clone()
    }

    pub fn config(&self) -> Config {
        self.config.clone()
    }

    pub async fn graceful_shutdown(&self, code: i32) -> Result<()> {
        info!("Gracefully shutting down...");
        self.api_shutdown.notify_waiters();
        self.api_shutdown_complete.notified().await;
        exit(code)
    }

    fn spawn_api(&self) -> JoinHandle<Result<()>> {
        tokio::spawn({
            let state = self.clone();
            async move {
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
            }
        })
    }

    fn watch_for_config_changes(&self) {
        tokio::spawn({
            let state = self.clone();
            async move {
                let mut config_changed = state.config.changed_from.clone();

                loop {
                    if let Err(err) = config_changed.changed().await {
                        error!("{err}");
                        break;
                    }

                    let _ = state.handle_config_change().await.inspect_err(|err| {
                        error!("An error occured while handling config change: {err}")
                    });
                }
            }
        });
    }

    pub async fn handle_config_change(&self) -> Result<()> {
        let config = self.config().get();
        let previous_config = (*self.config.changed_from.clone().borrow()).clone();

        let db_reload = tokio::spawn({
            let config = config.clone();
            let previous_config = previous_config.clone();
            let state = self.clone();
            async move {
                if config.database != previous_config.database {
                    if config.database.allow_config_reload_not_recommended {
                        warn!(
                            "Hot-reloading the database config is not recommended. Here be dragons!"
                        );
                        match database::create_database(&config.database).await {
                            Ok(new_database) => {
                                state.database.store(Arc::new(new_database));
                            }
                            Err(err) => {
                                error!("{err}")
                            }
                        }
                    } else {
                        info!("Hot-reloading database config disabled, ignoring config changes.")
                    }
                }
                if config.api != previous_config.api || config.graphql != previous_config.graphql {
                    state.api_shutdown.notify_waiters();
                    state.api_shutdown_complete.notified().await;
                    state.spawn_api();
                }
            }
        });

        let _ = tokio::join!(db_reload);

        Ok(())
    }
}
