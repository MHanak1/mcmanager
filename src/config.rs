use crate::database::types::Id;
use crate::util;
use log::debug;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::ops::Range;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub listen_address: String,
    pub listen_port: u16,
    pub api_rate_limit: f32,
    pub require_invite_to_register: bool,
    pub info: FrontendInfo,
    pub database: DatabaseConfig,
    pub minecraft_server_type: ServerType,
    pub remote: RemoteConfig,
    pub world: WorldConfig,
    pub user_defaults: UserDefaults,
    pub world_defaults: WorldDefaults,
    pub proxy: ProxyConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FrontendInfo {
    pub name: String,
    pub login_message: String,
    pub login_message_title: String,
    pub login_message_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub database_type: DatabaseType,
    pub cache_time_to_live: u64,
    pub max_connections: u32,
    pub pg_host: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseType {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerType {
    Internal,
    Remote,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyType {
    Infrarust,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteConfig {
    pub host: url::Url,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorldConfig {
    pub stop_timeout: u64,
    pub port_range: Range<u16>,
    pub java_launch_command: String,
    pub minimum_memory: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserDefaults {
    pub group_id: Id,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorldDefaults {
    /// Default amount of memory allocated to a server (in MiB)
    pub allocated_memory: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    pub port: u16,
    pub hostname: String,
    pub infrarust_executable_name: String,
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    let mut config_builder = config::Config::builder().add_source(config::File::from_str(
        &include_str!("resources/configs/default_config.toml").replace("$default_group_id", "AAAAAAAA"),
        config::FileFormat::Toml,
    ));

    let config_path = util::dirs::base_dir().join("config.toml");

    if config_path.exists() {
        debug!("loading config: {}", config_path.display());
        config_builder = config_builder.add_source(config::File::with_name(
            util::dirs::base_dir()
                .join("config.toml")
                .display()
                .to_string()
                .as_str(),
        ));
    }

    config_builder
        .build()
        .expect("failed to parse config")
        .try_deserialize::<Config>()
        .expect("failed to parse config")
});

pub mod secrets {
    use crate::util;
    use config::Config;
    use once_cell::sync::Lazy;
    

    pub struct Secrets {
        pub api_secret: String,
        pub forwarding_secret: String,
    }

    impl TryFrom<Config> for Secrets {
        type Error = color_eyre::eyre::Error;

        fn try_from(config: Config) -> Result<Self, Self::Error> {
            Ok(Self {
                api_secret: config.get("api_secret")?,
                forwarding_secret: config.get("forwarding_secret")?,
            })
        }
    }

    pub static SECRETS: Lazy<Secrets> = Lazy::new(|| {
        Secrets::try_from(
            Config::builder()
                .add_source(config::File::with_name(
                    util::dirs::base_dir()
                        .join("secrets.toml")
                        .display()
                        .to_string()
                        .as_str(),
                ))
                .build()
                .expect("failed to parse secrets"),
        )
        .expect("failed to parse secrets")
    });
}
