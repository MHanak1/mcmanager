use crate::util;
use log::debug;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::ops::Range;
use crate::database::types::Id;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub listen_address: String,
    pub listen_port: u32,
    pub public_routes_rate_limit: (u32, u64),
    pub private_routes_rate_limit: (u32, u64),
    pub minecraft_server_type: ServerType,
    pub internal: InternalConfig,
    pub remote: RemoteConfig,
    pub world: WorldConfig,
    pub user_defaults: UserDefaults,
    pub world_defaults: WorldDefaults,
    pub velocity: VelocityConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerType {
    Internal,
    Remote,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InternalConfig {
    pub launch_command: String,
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
pub struct VelocityConfig {
    pub port: u16,
    pub executable_name: String,
    pub hostname: String,
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    let config_path = util::dirs::base_dir().join("config.toml");

    if !config_path.exists() {
        let mut config_file = File::create(&config_path).expect("failed to create config file");
        config_file
            .write_all(include_bytes!("resources/default_config.toml"))
            .expect("failed to write default config file");
    }

    debug!(
        "loading config: {}",
        util::dirs::base_dir().join("config.toml").display()
    );

    let config = config::Config::builder()
        .add_source(config::File::with_name(
            util::dirs::base_dir()
                .join("config.toml")
                .display()
                .to_string()
                .as_str(),
        ))
        .build()
        .expect("failed to parse config");

    config
        .try_deserialize::<Config>()
        .expect("failed to parse config")
});

pub mod secrets {
    use crate::database::types::Token;
    use crate::util;
    use config::Config;
    use once_cell::sync::Lazy;

    pub struct Secrets {
        pub api_secret: Token,
    }

    impl TryFrom<Config> for Secrets {
        type Error = anyhow::Error;

        fn try_from(config: Config) -> Result<Self, Self::Error> {
            Ok(Self {
                api_secret: config.get("api_secret")?,
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
