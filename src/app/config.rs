use ::serde::Deserialize;
use ::serde::Serialize;
use arc_swap::ArcSwap;
use arc_swap::ArcSwapOption;
use color_eyre::Result;
use color_eyre::eyre::ContextCompat;
use color_eyre::eyre::bail;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use serde_with::serde_as;
use smart_default::SmartDefault;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::sync::watch::Sender;
use tokio::task::JoinHandle;
use tracing::warn;
use tracing::{debug, error, info};

use crate::app::paths::CONFIG;
use crate::app::paths::config_files;

#[derive(Serialize, Deserialize, SmartDefault, Debug, Clone, PartialEq)]
#[allow(unused)]
pub struct ConfigValues {
    pub database: Database,
}

#[serde_as]
#[derive(Serialize, Deserialize, SmartDefault, Debug, Clone, PartialEq)]
#[allow(unused)]
pub struct Database {
    #[default = "sqlite"]
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
}

impl ConfigValues {
    fn new() -> Result<Self> {
        let mut config = config::Config::builder();
        config = config.add_source(config::File::from_str(
            &serde_json::to_string(&ConfigValues::default())
                .expect("Failed to serialise the default config"),
            config::FileFormat::Json,
        ));
        config = config.add_source(config::Environment::with_prefix("MCM"));

        for path in config_files().iter() {
            config = config.add_source(
                config::File::with_name(
                    path.to_str()
                        .context("Could not convert config path to str")?,
                )
                .required(false),
            );
            debug!("Using config file {}", path.display());
        }

        Ok(config.build()?.try_deserialize()?)
    }
}

#[allow(unused)]
#[derive(Clone)]
pub struct Config {
    pub config: Arc<ArcSwap<ConfigValues>>,
    pub state: Arc<ConfigState>,
}

impl Config {
    pub fn new() -> Result<Self> {
        let config = Arc::new(ArcSwap::from_pointee(ConfigValues::new()?));
        let state = Arc::new(ConfigState::new(config.clone()));
        Ok(Self { config, state })
    }
}

pub const FILE_EXTENSIONS: [&str; 6] = ["toml", "json", "yaml", "yml", "ron", "json5"]; //a bad, not good way to do this

#[allow(unused)]
pub struct ConfigState {
    task: JoinHandle<()>,
    pub config_changed: watch::Sender<ConfigValues>,
    pub previous_config: Arc<ArcSwapOption<ConfigValues>>,
}

impl ConfigState {
    fn new(config: Arc<ArcSwap<ConfigValues>>) -> Self {
        let (config_changed, _): (Sender<ConfigValues>, _) =
            watch::channel((*config.load_full()).clone());
        let previous_config = Arc::new(ArcSwapOption::from_pointee(None));

        let task = tokio::task::spawn({
            let config_changed = config_changed.clone();
            let previous_config: Arc<ArcSwapOption<ConfigValues>> = previous_config.clone();

            async move {
                // Create a channel to receive the events.
                let (tx, rx) = mpsc::channel();

                // Automatically select the best implementation for your platform.
                // You can also access each implementation directly e.g. INotifyWatcher.
                let mut watcher = new_debouncer(Duration::from_secs(1), None, tx)
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
                    match rx.recv() {
                        Ok(Ok(events)) => {
                            'events: for event in events {
                                match event.event {
                                    notify::Event {
                                        kind:
                                            notify::EventKind::Modify(_) | notify::EventKind::Remove(_),
                                        ..
                                    } => {
                                        info!("Config changed, refreshing.");
                                        match ConfigValues::new() {
                                            Err(err) => {
                                                error!("Unable to deserialise config: {}", err);
                                            }
                                            Ok(new_config) => {
                                                if **config.load() != new_config {
                                                    debug!("New config: {new_config:?}");
                                                    //previous_config.store(Arc::new(Some(
                                                    //    config.swap(Arc::new(new_config)).as_ref(),
                                                    //)));
                                                    previous_config.store(Some(Arc::new(
                                                        (*config
                                                            .swap(Arc::new(new_config.clone())))
                                                        .clone(),
                                                    )));

                                                    println!("notifying");
                                                    config_changed
                                                        .send(new_config)
                                                        .expect("dunno man");
                                                    break 'events;
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Err(err) => error!("Unable to deserialise config: {}", err),
                        _ => {}
                    }
                }
            }
        });

        Self {
            task,
            config_changed,
            previous_config,
        }
    }
}

pub fn generate_config_file(path: impl AsRef<Path>) -> Result<()> {
    if path.as_ref().exists() {
        // not *the* best way to do it, but certainly a way.
        warn!("Can not create a config file. File already exists");
        return Ok(());
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
