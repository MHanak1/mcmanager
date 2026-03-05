use color_eyre::eyre::Result;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tracing::{debug, error, warn};

use crate::app::{self, config::Config, paths::DATA_DIR};

#[derive(Clone)]
pub struct State {
    pub database: DatabaseConnection,
    pub config: Config,
}

impl State {
    pub async fn new() -> Result<Self> {
        let config = Config::new()?;
        let dbconfig = &config.values.load().database.clone();
        let database = Self::create_database(dbconfig).await?;

        let state = Self { config, database };
        state.watch_for_config_changes();
        state.watch_for_config_changes(); // i hate tokio
        // for some f-ing reason one thread will never work. if we have two tasks, one of them is guaranteed to work

        Ok(state)
    }

    fn watch_for_config_changes(&self) {
        tokio::spawn({
            let state = self.clone();
            async move {
                let mut config_changed = state.config.changed_from.clone();

                loop {
                    //use tokio::time;
                    //time::sleep(Duration::from_secs(1)).await; //this works
                    config_changed.changed().await.expect("dunno man"); //this doesn't

                    let _ = state.handle_config_change().await.inspect_err(|err| {
                        error!("An error occured while handling config change: {err}")
                    });
                }
            }
        });
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
