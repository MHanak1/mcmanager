use std::sync::Arc;

use color_eyre::eyre::Result;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tokio::task::JoinHandle;
use tracing::{debug, error, warn};

use crate::app::{self, config::Config, paths::DATA_DIR};

#[derive(Clone)]
pub struct State {
    pub database: DatabaseConnection,
    pub config: Config,
    config_change_handler: Option<Arc<JoinHandle<()>>>,
}

impl State {
    pub async fn new() -> Result<Self> {
        let config = Config::new()?;
        let dbconfig = &config.values.load().database.clone();
        let database = Self::create_database(dbconfig).await?;
        let config_change_handler = None;

        let mut state = Self {
            config,
            database,
            config_change_handler,
        };

        state.config_change_handler = Some(Arc::new(tokio::spawn({
            let state = state.clone();
            async move {
                let mut config_changed = state.config.changed_from.clone();

                loop {
                    println!("State czeka na zmiany configu");
                    config_changed.changed().await.expect("dunno man");
                    println!("State widzi zmiany configu");

                    let _ = state.handle_config_change().await.inspect_err(|err| {
                        error!("An error occured while handling config change: {err}")
                    });
                }
            }
        })));

        Ok(state)
    }

    pub async fn handle_config_change(&self) -> Result<()> {
        let config = self.config.values.load_full();
        let previous_config = (*self.config.changed_from.clone().borrow_and_update()).clone();

        if config.database != previous_config.database {
            warn!("Changing database config on the fly not supported. Ignoring.")
        }

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

        let a = Ok(Database::connect(opt).await?);
        a
    }
}
