use std::{process::exit, sync::Arc};

use arc_swap::ArcSwap;
use color_eyre::eyre::Result;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::json;
use tokio::sync::Notify;
use tracing::{error, info, warn};

use crate::{api, app::config::Config, database};

#[derive(Clone)]
pub struct AppState {
    database: Arc<ArcSwap<DatabaseConnection>>,
    config: Config,

    // == Signals (yes i did godot, fight me) ==
    pub api_shutdown: Arc<tokio::sync::Notify>,
    pub api_shutdown_complete: Arc<tokio::sync::Notify>,
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
        api::serve(state.clone());

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
                    api::serve(state.clone());
                }
            }
        });

        let _ = tokio::join!(db_reload);

        Ok(())
    }
}
