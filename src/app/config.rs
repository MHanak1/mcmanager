use ::serde::Deserialize;
use ::serde::Serialize;
use arc_swap::ArcSwap;
use color_eyre::Result;
use color_eyre::eyre::ContextCompat;
use color_eyre::eyre::bail;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use regex::Regex;
use serde_with::serde_as;
use smart_default::SmartDefault;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tracing::warn;
use tracing::{debug, error, info};
use validator::Validate;

use crate::app::paths::CONFIG;
use crate::app::paths::config_files;

static RE_DATABASE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(sqlite|((sqlite|postgres|mysql)(:.+)?))$").expect("Invalid Regex")
});

#[derive(Serialize, Deserialize, SmartDefault, Debug, Clone, PartialEq, Validate)]
#[allow(unused)]
pub struct ConfigValues {
    #[validate(nested)]
    pub app: App,
    #[validate(nested)]
    pub database: Database,
    #[validate(nested)]
    pub graphql: GraphQL,
    #[validate(nested)]
    pub api: Api,
}

#[derive(Serialize, Deserialize, SmartDefault, Debug, Clone, PartialEq, Validate)]
#[allow(unused)]
pub struct App {
    #[default = false]
    pub require_invite_for_account_creation: bool,
}

#[serde_as]
#[derive(Serialize, Deserialize, SmartDefault, Debug, Clone, PartialEq, Validate)]
#[allow(unused)]
pub struct Database {
    #[default = "sqlite"]
    #[validate(regex(path = *RE_DATABASE))]
    pub connection: String,

    #[default = 100]
    pub max_connections: u32,

    #[default = 5]
    pub min_connections: u32,

    #[default(Duration::from_secs(10))]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    pub connect_timeout: Duration,

    #[default(Duration::from_secs(10))]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    pub acquire_timeout: Duration,

    #[default(Duration::from_secs(10))]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    pub idle_timeout: Duration,

    #[default(Duration::from_secs(10))]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    pub max_lifetime: Duration,

    #[default = true]
    pub sqlx_logging: bool,

    #[default = false]
    /// Allow for hot reloading of the database connection. This is generally not the best idea.
    pub allow_config_reload_not_recommended: bool,
}

#[derive(Serialize, Deserialize, SmartDefault, Debug, Clone, PartialEq, Validate)]
#[allow(unused)]
pub struct Api {
    #[default(String::from("0.0.0.0:3000"))]
    pub bind: String,
}

#[derive(Serialize, Deserialize, SmartDefault, Debug, Clone, PartialEq, Validate)]
pub struct GraphQL {
    #[default(String::from("/api/graphql"))]
    pub endpoint: String,
    #[default(Some(10))]
    pub depth: Option<usize>,
    #[default(Some(100))]
    pub complexity: Option<usize>,
    #[default = false]
    pub playground: bool,
}

impl ConfigValues {
    fn new() -> Result<Self> {
        let mut config = config::Config::builder();
        config = config.add_source(config::File::from_str(
            &serde_json::to_string(&ConfigValues::default())
                .expect("Failed to serialise the default config"),
            config::FileFormat::Json,
        ));

        let files = config_files();
        for path in files.iter() {
            config = config.add_source(
                config::File::with_name(
                    path.to_str()
                        .context("Could not convert config path to str")?,
                )
                .required(false),
            );
            debug!("Using config file {}", path.display());
        }

        if files.len() == 1 && !files.first().unwrap().is_file() {
            warn!(
                "Comfig file not found at {} not found. You can generate a config using the --gen-config parameter",
                files.first().unwrap().display()
            )
        }

        config = config.add_source(config::Environment::with_prefix("MCM").separator("_"));

        let config: ConfigValues = config.build()?.try_deserialize()?;

        config.validate()?;

        Ok(config)
    }
}

#[allow(unused)]
#[derive(Clone)]
pub struct Config {
    values: Arc<ArcSwap<ConfigValues>>,
    /// This value updates whenever config changes, and emits what the config changed from.
    pub changed_from: watch::Receiver<Arc<ConfigValues>>,
}

pub const FILE_EXTENSIONS: [&str; 6] = ["toml", "json", "yaml", "yml", "ron", "json5"]; //a bad, not good way to do this

impl Config {
    pub fn new() -> Result<Self> {
        let values = Arc::new(ArcSwap::from_pointee(ConfigValues::new()?));
        let changed_from = Self::spawn_watcher(values.clone());

        Ok(Self {
            values,
            changed_from,
        })
    }

    pub fn get(&self) -> Arc<ConfigValues> {
        self.values.load_full()
    }

    fn spawn_watcher(config: Arc<ArcSwap<ConfigValues>>) -> watch::Receiver<Arc<ConfigValues>> {
        let (config_changed_tx, config_change_rx) = watch::channel(config.load_full()).clone();

        tokio::task::spawn({
            async move {
                // Create a channel to receive the events.
                let (tx, mut rx) = mpsc::channel(16);

                // Automatically select the best implementation for your platform.
                // You can also access each implementation directly e.g. INotifyWatcher.
                let mut watcher = new_debouncer(Duration::from_secs(1), None, move |events| {
                    if let Err(err) = tx.try_send(events) {
                        error!("Watch channel send error: {:?}", err);
                    }
                })
                .expect("Failed to set up config file watcher");

                if CONFIG.exists() {
                    match watcher.watch(&*CONFIG, RecursiveMode::NonRecursive) {
                        Ok(_) => {
                            info!("Watching for config changes in {}", &CONFIG.display());
                        }
                        Err(err) => {
                            warn!(
                                "Failed to watch for config changes in {}: {}",
                                &CONFIG.display(),
                                err
                            )
                        }
                    }
                }
                loop {
                    while let Some(events) = rx.recv().await {
                        if let Ok(events) = events {
                            'events: for event in events {
                                if let notify::Event {
                                    kind:
                                        notify::EventKind::Modify(_) | notify::EventKind::Remove(_),
                                    ..
                                } = event.event
                                {
                                    match ConfigValues::new() {
                                        Err(err) => {
                                            error!("Unable to deserialise config: {}", err);
                                        }
                                        Ok(new_config) => {
                                            if **config.load() == new_config {
                                                debug!("Config written to, but it was not changed")
                                            } else {
                                                info!("Config changed, refreshing.");
                                                debug!("New config: {new_config:?}");
                                                //previous_config.store(Arc::new(Some(
                                                //    config.swap(Arc::new(new_config)).as_ref(),
                                                //)));
                                                let previous_config =
                                                    config.swap(Arc::new(new_config));

                                                if let Err(err) =
                                                    config_changed_tx.send(previous_config)
                                                {
                                                    error!(
                                                        "Failed when notifying about config change: {err}"
                                                    );
                                                }
                                                break 'events;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        config_change_rx
    }

    pub fn generate_config_file(path: impl AsRef<Path>) -> Result<()> {
        if path.as_ref().exists() {
            bail!(
                "Can not create a config file. File already exists ({})",
                path.as_ref().display()
            );
        }

        let extension = path
            .as_ref()
            .extension()
            .context("Cannot generate a config file. Config file name missing extension.")?;

        let config = match extension
            .to_str()
            .context("Could not convert config file extension to str")?
        {
            "toml" => toml::to_string_pretty(&ConfigValues::default())?,
            "json" => serde_json::to_string_pretty(&ConfigValues::default())?,
            "json5" => serde_json5::to_string(&ConfigValues::default())?,
            "yaml" | "yml" => serde_yaml::to_string(&ConfigValues::default())?,
            "ron" => {
                ron::ser::to_string_pretty(&ConfigValues::default(), ron::ser::PrettyConfig::new())?
            }
            _ => {
                bail!(
                    "extension {} not supported. Allowed config file types are {}",
                    extension.display(),
                    FILE_EXTENSIONS.join(", "),
                );
            }
        };

        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(path)?;
        file.write_all(config.as_bytes())?;
        Ok(())
    }
}
