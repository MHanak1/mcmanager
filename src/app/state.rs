use std::{sync::Arc, time::Duration};

use pretty_assertions::{assert_eq, assert_ne};

use color_eyre::eyre::Result;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tokio::task::JoinHandle;
use tracing::{debug, error};

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
        let dbconfig = &config.config.load().database.clone();
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
                let mut config_changed = state.config.state.config_changed.subscribe();

                loop {
                    println!("Printuje");
                    config_changed.changed().await.expect("dunno man");
                    println!("Nie printuje");

                    let _ = state.handle_config_change().await.inspect_err(|err| {
                        error!("An error occured while handling config change: {err}")
                    });
                }
            }
        })));

        Ok(state)
    }

    pub async fn handle_config_change(&self) -> Result<()> {
        println!("{:?}", self.config.config.load().database,);
        println!(
            "{:?}",
            self.config
                .state
                .previous_config
                .load()
                .clone()
                .unwrap()
                .database
        );
        assert_eq!(
            self.config.config.load().database,
            self.config
                .state
                .previous_config
                .load()
                .clone()
                .unwrap()
                .database
        );

        Ok(())
    }

    pub async fn create_database(config: &app::config::Database) -> Result<DatabaseConnection> {
        let url = if config.connection == "sqlite" {
            String::from(format!(
                "sqlite://{}/database.sqlite?mode=rwc",
                DATA_DIR.display()
            ))
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
