use crate::util;
use once_cell::sync::Lazy;
use serde::{Deserialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::ops::Range;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub listen_address: String,
    pub listen_port: u32,
    pub public_routes_rate_limit: (u32, u64),
    pub private_routes_rate_limit: (u32, u64),
    pub minecraft_server_type: String,
    pub internal: InternalConfig,
    pub remote: RemoteConfig,
    pub world: WorldConfig,
    pub user_defaults: UserDefaults,
    pub world_defaults: WorldDefaults,
    pub velocity: VelocityConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InternalConfig {
    pub launch_command: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteConfig {
    pub address: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorldConfig {
    pub stop_timeout: u64,
    pub port_range: Range<u16>,
    pub java_launch_command: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserDefaults {
    /// total RAM limit ( in MiB)
    pub memory_limit: u32,
    /// total wolds the user can have enabled
    pub world_limit: u32,
    /// amount of worlds that user can have enabled at a time
    pub active_world_limit: u32,
    /// total amount of storage available to the user ( in MiB)
    pub storage_limit: u32,

    pub config_blacklist: Vec<String>,
    pub config_whitelist: Vec<String>,
    pub config_limits: HashMap<String, crate::minecraft::server::ServerConfigLimit>,
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
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    let config_path = util::dirs::base_dir().join("config.toml");

    if !config_path.exists() {
        let mut config_file = File::create(&config_path).expect("failed to create config file");
        config_file
            .write_all(include_bytes!("resources/default_config.toml"))
            .expect("failed to write default config file");
    }

    let config = config::Config::builder()
        .add_source(config::File::from_str(
            include_str!("resources/default_config.toml"),
            config::FileFormat::Toml,
        ))
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
