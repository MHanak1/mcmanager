use anyhow::Context;
use once_cell::sync::Lazy;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_address: String,
    pub listen_port: u32,
    pub login_rate_limit: Option<u32>,
    pub user_defaults: UserDefaults,
    pub world_defaults: WorldDefaults,
}

impl TryFrom<config::Config> for Config {
    type Error = anyhow::Error;
    fn try_from(value: config::Config) -> Result<Self, Self::Error> {
        Ok(Self {
            listen_address: value.get_string("listen_address")?,
            listen_port: value.get_int("listen_port")? as u32,
            login_rate_limit: value.get_int("login_rate_limit")?.try_into().ok(),
            user_defaults: UserDefaults::try_from(value.get_table("user_defaults")?)?,
            world_defaults: WorldDefaults::try_from(value.get_table("world_defaults")?)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct UserDefaults {
    /// total RAM limit ( in MiB)
    pub memory_limit: u32,
    /// Per-world player limit
    pub player_limit: u32,
    /// total wolds the user can have enabled
    pub world_limit: u32,
    /// amount of worlds that user can have enabled at a time
    pub active_world_limit: u32,
    /// total amount of storage available to the user ( in MiB)
    pub storage_limit: u32,
}

impl TryFrom<HashMap<String, config::Value>> for UserDefaults {
    type Error = anyhow::Error;

    fn try_from(value: HashMap<String, config::Value>) -> Result<Self, Self::Error> {
        Ok(Self {
            memory_limit: value
                .get("memory_limit")
                .context("couldn't get memory_limit")?
                .clone()
                .into_int()? as u32,
            player_limit: value
                .get("player_limit")
                .context("couldn't get player_limit")?
                .clone()
                .into_int()? as u32,
            world_limit: value
                .get("world_limit")
                .context("couldn't get world_limit")?
                .clone()
                .into_int()? as u32,
            active_world_limit: value
                .get("active_world_limit")
                .context("couldn't get active_world_limit")?
                .clone()
                .into_int()? as u32,
            storage_limit: value
                .get("storage_limit")
                .context("couldn't get active_world_limit")?
                .clone()
                .into_int()? as u32,
        })
    }
}

#[derive(Debug, Clone)]
pub struct WorldDefaults {
    /// Default amount of memory allocated to a server (in MiB)
    pub allocated_memory: u32,
}

impl TryFrom<HashMap<String, config::Value>> for WorldDefaults {
    type Error = anyhow::Error;
    fn try_from(value: HashMap<String, config::Value>) -> Result<Self, Self::Error> {
        Ok(Self {
            allocated_memory: value
                .get("allocated_memory")
                .context("couldn't get allocated_memory")?
                .clone()
                .into_int()? as u32,
        })
    }
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    Config::try_from(
        config::Config::builder()
            .add_source(config::File::from_str(
                include_str!("resources/default_config.toml"),
                config::FileFormat::Toml,
            ))
            .build()
            .expect("failed to parse config"),
    )
    .expect("failed to parse config")
});
