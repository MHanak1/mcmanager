use std::sync::Arc;

use arc_swap::ArcSwap;
use color_eyre::eyre::Result;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tracing::{debug, error, info, warn};

use crate::app::{self, config::Config, paths::DATA_DIR};

#[derive(Clone)]
pub struct State {
    database: Arc<ArcSwap<DatabaseConnection>>,
    config: Config,
}

impl State {
    pub async fn new() -> Result<Self> {
        let config = Config::new()?;
        let dbconfig = &config.get().database.clone();
        let database = Arc::new(ArcSwap::from_pointee(
            Self::create_database(dbconfig).await?,
        ));

        let state = Self { config, database };
        state.watch_for_config_changes();
        state.watch_for_config_changes(); // i hate tokio
        // for some f-ing reason one task will never work. if we have two tasks, one of them is guaranteed to work

        Ok(state)
    }

    pub fn database(&self) -> DatabaseConnection {
        (*self.database.load_full()).clone()
    }

    pub fn config(&self) -> Config {
        self.config.clone()
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
                        match Self::create_database(&config.database).await {
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
            }
        });

        let _ = tokio::join!(db_reload);

        Ok(())
    }

    pub async fn create_database(config: &app::config::Database) -> Result<DatabaseConnection> {
        let url = if config.connection == "sqlite" {
            format!("sqlite://{}/database.sqlite?mode=rwc", DATA_DIR.display())
        } else {
            config.connection.clone()
        };
        debug!("Connecting to database at: {}", url);

        let mut opt = ConnectOptions::new(url);
        opt.max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .connect_timeout(config.connect_timeout)
            .acquire_timeout(config.acquire_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .sqlx_logging_level(log::LevelFilter::Debug);

        let db = Database::connect(opt).await?;
        db.get_schema_registry("mcmanager::entity::*")
            .sync(&db)
            .await?;

        Ok(db)
    }
}
