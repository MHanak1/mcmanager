use crate::config::{CONFIG, ServerType};
use crate::database::objects::World;
use crate::database::types::Id;
use crate::minecraft;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt::Debug;
use std::result;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

pub type ServerMutex = Arc<Mutex<Box<dyn MinecraftServer>>>;
static SERVERS: LazyLock<Mutex<HashMap<Id, ServerMutex>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub world: World,
    pub status: MinecraftServerStatus,
    pub port: Option<u16>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum MinecraftServerStatus {
    Running,
    Exited(u32),
}

/*
pub mod config {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Difficulty {
        Peaceful,
        Easy,
        Normal,
        Hard,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Gamemode {
        Survival,
        Creative,
        Adventure,
        Spectator,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub struct MinecraftServerConfig {
        allow_flight: bool,
        allow_nether: bool,
        broadcast_console_to_ops: bool,
        broadcast_rcon_to_ops: bool,
        bug_report_link: String,
        difficulty: Difficulty,
        enable_command_block: bool,
        enable_jmx_monitoring: bool,
        enable_query: bool,
        // don't allow rcon, wa ain't got ports for that
        enforce_secure_profile: bool,
        enforce_whitelist: bool,
        entity_brodcast_range_percentage: u32,
        force_gamemode: bool,
        function_permission_level: u32,
        gamemode: Gamemode,
        generate_structures: bool,
        generator_settings: String,
        hardcore: bool,
        hide_online_players: bool,
        initial_disabled_packs: String,
        initial_enabled_packs: String,
        level_name: String,
        level_seed: String,
        level_type: String,
        log_ips: bool,
        max_chained_neighbor_updates: u32,
        // don't allow to change max_players
        max_tick_time: u32,
        max_world_size: u32,
        motd: String,
        network_compression_threshold: u32,
        // don't allow to change online-mode, this would break velocity forwarding.
        op_permission_level: u32,
        pause_whan_empty_seconds: u32,
        player_idle_timeout: u32,
        pvp: bool,
        rate_limit: u32,
        require_resource_pack: bool,
        resource_pack: String,
        resource_pack_id: String,
        resource_pack_prompt: String,
        resource_pack_sha1: String,
        // don't allow to change server-ip or server-port
        simulation_distance: u32,
        spawn_monsters: bool,
        spawn_protection: u32,
        sync_chunk_writes: bool,
        text_filtering_config: String,
        text_filtering_version: u32,
        use_native_transport: bool,
        view_distance: u32,
        white_list: bool,
    }
}
 */

pub async fn get_server(id: Id) -> Option<ServerMutex> {
    SERVERS.lock().await.get(&id).cloned()
}

pub async fn get_or_create_server(world: &World) -> Result<minecraft::server::ServerMutex> {
    let mut server = get_server(world.id).await;
    match server {
        Some(server) => Ok(server),
        None => {
            add_server(match CONFIG.minecraft_server_type {
                ServerType::Internal => Box::new(
                    internal::InternalServer::new(world.clone())
                        .await
                        .map_err(|err| {
                            crate::database::DatabaseError::InternalServerError(err.to_string())
                        })?,
                ),
                ServerType::Remote => Box::new(external::MinimanagerServer::new(
                    CONFIG
                        .remote
                        .host
                        .host()
                        .expect("invalid remote server hostname")
                        .to_string(),
                    world.clone(),
                )),
            })
            .await;
            server = get_server(world.id).await;
            assert!(server.is_some());
            Ok(server.unwrap())
        }
    }
}

pub async fn add_server(server: Box<dyn MinecraftServer>) {
    SERVERS
        .lock()
        .await
        .insert(server.id(), Arc::new(Mutex::new(server)));
}

pub async fn remove_server(id: &Id) {
    SERVERS.lock().await.remove(id);
}

pub async fn get_all_servers() -> Vec<ServerMutex> {
    SERVERS.lock().await.values().cloned().collect()
}

pub async fn get_all_worlds() -> Vec<World> {
    futures::future::join_all(
        SERVERS
            .lock()
            .await
            .values()
            .map(|server: &ServerMutex| async { server.lock().await.world() }),
    )
    .await
    /*
    SERVERS
        .lock()
        .await
        .values()
        .filter_map(async |server: &ServerMutex| server.lock().await.world().await.ok())
        .collect()
     */
}

/*
pub trait MinecraftServer: Send {
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn id(&self) -> Id;
    fn world_and_status(&self) -> Result<Server>;
    fn status(&self) -> MinecraftServerStatus;
    fn world(&self) -> World;
    fn port(&self) -> Option<u16>;
    fn host(&self) -> String;
    fn set_world(&mut self, world: World) -> Result<()>;
    fn refresh(&mut self);
    fn read_console(&mut self) -> Result<String>;
    fn write_console(&mut self, data: &[u8]) -> Result<()>;
    fn read_file(&self, path: &str) -> Result<String>;
    fn write_file(&self, path: &str, data: &str) -> Result<()>;
    fn remove_file(&self, path: &str) -> Result<()>;
}
 */

#[async_trait]
pub trait MinecraftServer: Send {
    fn id(&self) -> Id;
    fn world(&self) -> World;
    /// to which port is the server bound to
    fn port(&self) -> Option<u16>;
    /// under what subdomain is the server avaliable)
    fn host(&self) -> String;
    /// Where the minecraft server resides
    fn hostname(&self) -> Option<String>;
    async fn update_world(&mut self, world: World) -> Result<()>;
    async fn config(&self) -> Result<HashMap<String, String>>;
    async fn set_config(&mut self, config: HashMap<String, String>) -> Result<()>;
    async fn status(&self) -> Result<MinecraftServerStatus>;
    /// fully removes the server and its files
    async fn remove(&mut self) -> Result<()>;
    /// updates the status of the server. this should return false if the server is updated through somewhere else
    async fn refresh(&mut self) -> bool;
}

pub mod internal {
    use crate::config::CONFIG;
    use crate::database::objects::World;
    use crate::database::types::Id;
    use crate::minecraft::server::{MinecraftServer, MinecraftServerStatus};
    use crate::util;
    use anyhow::Result;
    use anyhow::{Context, bail};
    use async_trait::async_trait;
    use log::{debug, info, warn};
    use std::collections::{HashMap, HashSet};
    use std::fs::File;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};
    use std::time::Duration;
    use subprocess::{Exec, ExitStatus, Popen};

    pub(crate) static TAKEN_LOCAL_PORTS: LazyLock<Mutex<HashSet<u16>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));

    fn get_free_local_port() -> Option<u16> {
        let servers = TAKEN_LOCAL_PORTS.lock().expect("couldn't get servers");
        crate::config::CONFIG
            .world
            .port_range
            .clone()
            .find(|&port| !servers.contains(&port) && port != CONFIG.velocity.port)
    }

    pub struct InternalServer {
        status: MinecraftServerStatus,
        world: World,
        directory: PathBuf,
        port: Option<u16>,
        hostname: String,
        process: Option<Popen>,
    }
    impl InternalServer {
        pub async fn new(world: World) -> Result<Self> {
            let enabled = world.enabled;
            let mut new = Self {
                status: MinecraftServerStatus::Exited(0),
                hostname: world.hostname.clone(),
                directory: util::dirs::worlds_dir()
                    .join(format!("{}/{}", world.owner_id, world.id)),
                port: None,
                process: None,
                world,
            };
            if enabled {
                new.start().await?;
            }
            Ok(new)
        }

        async fn initialise_files(&self) -> Result<()> {
            let port = self
                .port()
                .context("Port not set, cannot initialise files")?;

            let properties = self
                .read_file("server.properties")
                .unwrap_or(include_str!("../resources/server.properties").to_string());
            let mut properties = crate::minecraft::util::parse_minecraft_properties(&properties);
            properties.insert(String::from("query.port"), format!("{port}"));
            properties.insert(String::from("server-port"), format!("{port}",));

            let properties = crate::minecraft::util::create_minecraft_properties(properties);
            self.write_file("server.properties", &properties)?;

            self.write_file("eula.txt", "eula=true")?;

            Ok(())
        }
    }

    impl InternalServer {
        async fn start(&mut self) -> Result<()> {
            let jar_path =
                util::dirs::versions_dir().join(format!("{}.jar", self.world.version_id));
            if !jar_path.exists() {
                bail!("{} doesn't exist", jar_path.display());
            }

            if !self.directory.exists() {
                std::fs::create_dir_all(&self.directory)?;
            }

            if self.process.is_some() {
                debug!("server already running");
                return Ok(());
            }

            let port = get_free_local_port().context("No free ports left")?;
            info!("assigning port {} for {}", port, self.world.id);
            self.port = Some(port);
            TAKEN_LOCAL_PORTS
                .lock()
                .expect("failed to lock local ports")
                .insert(port);

            self.initialise_files().await?;
            debug!("starting server {}", self.id());
            let command = CONFIG.world.java_launch_command.clone();
            let command = command.replace("%jar%", jar_path.display().to_string().as_str());
            let command = command.replace(
                "%max_mem%",
                &format!("-Xms{}m", CONFIG.world.minimum_memory),
            );
            let command = command.replace(
                "%max_mem%",
                &format!("-Xmx{}m", self.world.allocated_memory),
            );
            //println!("{command}");
            let command = Exec::shell(command)
                .cwd(self.directory.clone())
                .stdin(subprocess::Redirection::Pipe)
                .stdout(subprocess::Redirection::Pipe)
                .stderr(subprocess::Redirection::Pipe)
                .popen()
                .inspect_err(|_| {
                    self.status = MinecraftServerStatus::Exited(1);
                })
                .expect(":<");

            self.process = Some(command);

            self.status = MinecraftServerStatus::Running;

            Ok(())
        }

        async fn stop(&mut self) -> Result<()> {
            let stop_result = self.write_console(b"stop\n");
            if let Some(mut process) = self.process.take() {
                if stop_result.is_err() {
                    let _ = process.kill();
                }

                let result = process.wait_timeout(Duration::from_secs(
                    crate::config::CONFIG.world.stop_timeout,
                ))?;
                TAKEN_LOCAL_PORTS
                    .lock()
                    .expect("failed to lock local ports")
                    .remove(&self.port.unwrap_or(0));
                self.port = None;

                match result {
                    Some(status) => {
                        info!("stopped server {} with status {:?}", self.id(), status);
                        if let ExitStatus::Exited(code) = status {
                            self.status = MinecraftServerStatus::Exited(code);
                        } else {
                            self.status = MinecraftServerStatus::Exited(1);
                        }
                        Ok(())
                    }
                    None => {
                        process.terminate()?;
                        warn!("stopped server {} after timeout period elapsed", self.id());
                        self.status = MinecraftServerStatus::Exited(1);
                        Ok(())
                    }
                }
            } else {
                debug!("Not stopping {}, because it's not running", self.world.id);
                Ok(())
            }
        }

        /*
        fn read_console(&mut self) -> Result<String> {
            let mut output = String::new();

            match self.process {
                Some(ref mut process) => match process.stdout {
                    Some(ref mut stdout) => {
                        stdout.read_to_string(&mut output)?;
                    }
                    None => {
                        bail!("Could not get process stdout");
                    }
                },
                None => {
                    match self.status {
                        MinecraftServerStatus::Exited(_) => {}
                        _ => {
                            self.status = MinecraftServerStatus::Exited(1);
                        }
                    }
                    bail!("Cannot read console: server is not running");
                }
            }
            Ok(output)
        }
         */

        fn write_console(&mut self, data: &[u8]) -> Result<()> {
            match self.process {
                Some(ref mut process) => match process.stdin {
                    Some(ref mut stdin) => {
                        stdin.write_all(data)?;
                    }
                    None => {
                        bail!("Could not get process stdin");
                    }
                },
                None => {
                    match self.status {
                        MinecraftServerStatus::Exited(_) => {}
                        _ => {
                            self.status = MinecraftServerStatus::Exited(1);
                        }
                    }
                    bail!("Cannot write to console: server is not running");
                }
            }
            Ok(())
        }

        fn read_file(&self, path: &str) -> Result<String> {
            let mut output = String::new();
            let mut file = File::open(self.directory.join(path))?;
            file.read_to_string(&mut output)?;
            Ok(output)
        }

        fn write_file(&self, path: &str, data: &str) -> Result<()> {
            let mut file = File::create(self.directory.join(path))?;
            file.write_all(data.as_bytes())?;
            Ok(())
        }

        /*
        fn remove_file(&self, path: &str) -> Result<()> {
            std::fs::remove_file(self.directory.join(path))?;
            Ok(())
        }
         */
    }

    #[async_trait]
    impl MinecraftServer for InternalServer {
        fn id(&self) -> Id {
            self.world.id
        }

        fn world(&self) -> World {
            self.world.clone()
        }

        fn port(&self) -> Option<u16> {
            self.port
        }

        fn host(&self) -> String {
            CONFIG.listen_address.clone()
        }

        fn hostname(&self) -> Option<String> {
            Some(self.hostname.clone())
        }

        async fn update_world(&mut self, world: World) -> Result<()> {
            let old = self.world();
            if old.allocated_memory != world.allocated_memory || old.version_id != world.version_id
            {
                self.stop().await?;
            }

            let enabled = world.enabled;

            self.hostname = world.hostname.clone();
            self.world = world;

            if enabled {
                self.start().await?;
            } else {
                self.stop().await?;
            }
            Ok(())
        }

        async fn config(&self) -> Result<HashMap<String, String>> {
            let properties = self
                .read_file("server.properties")
                .unwrap_or(include_str!("../resources/server.properties").to_string());
            Ok(crate::minecraft::util::parse_minecraft_properties(
                &properties,
            ))
        }

        async fn set_config(&mut self, config: HashMap<String, String>) -> Result<()> {
            let properties = crate::minecraft::util::create_minecraft_properties(config);
            self.write_file("server.properties", &properties)?;
            Ok(())
        }

        async fn status(&self) -> Result<MinecraftServerStatus, anyhow::Error> {
            Ok(self.status)
        }

        async fn remove(&mut self) -> Result<()> {
            self.stop().await?;
            debug!("removing directory {}", self.directory.display());
            if self.directory.exists() {
                std::fs::remove_dir_all(self.directory.clone())?;
            }
            Ok(())
        }

        async fn refresh(&mut self) -> bool {
            if let Some(process) = self.process.as_mut() {
                if let Some(exit_status) = process.poll() {
                    self.process = None;
                    match exit_status {
                        ExitStatus::Exited(code) => {
                            self.status = MinecraftServerStatus::Exited(code);
                        }
                        _ => {
                            self.status = MinecraftServerStatus::Exited(1);
                        }
                    }
                    info!(
                        "freed the port {} of {} because the server running on it has exited",
                        self.port.unwrap_or(0),
                        self.world.id
                    );
                    TAKEN_LOCAL_PORTS
                        .lock()
                        .expect("failed to lock local ports")
                        .remove(&self.port.unwrap_or(0));
                }
            }
            true
        }
    }

    impl Drop for InternalServer {
        fn drop(&mut self) {
            if let Some(mut process) = self.process.take() {
                process.kill().expect("Failed to kill process");
            }
        }
    }
}

pub mod external {
    use crate::config::CONFIG;
    use crate::database::objects::World;
    use crate::database::types::Id;
    use crate::minecraft::server::{MinecraftServer, MinecraftServerStatus, Server};
    use anyhow::__private::kind::TraitKind;
    use anyhow::{Result, bail};
    use async_trait::async_trait;
    use log::debug;
    use reqwest::StatusCode;
    use std::collections::HashMap;

    pub struct MinimanagerServer {
        host: String,
        port: Option<u16>,
        hostname: String,
        world: World,
    }

    impl MinimanagerServer {
        pub fn new(host: String, world: World) -> Self {
            Self {
                hostname: world.hostname.clone(),
                host,
                port: None,
                world,
            }
        }

        pub async fn server(&self) -> Result<Server> {
            debug!("Requesting minimanager to update server");
            let client = reqwest::Client::new();
            Ok(serde_json::from_str(
                &client
                    .post(format!("{}api/worlds", CONFIG.remote.host.to_string()))
                    .header(
                        "Authorization",
                        format!("Bearer {}", crate::config::secrets::SECRETS.api_secret),
                    )
                    .body(serde_json::to_string(&self.world).unwrap())
                    .send()
                    .await?
                    .text()
                    .await?,
            )?)
        }
    }

    #[async_trait]
    impl MinecraftServer for MinimanagerServer {
        fn id(&self) -> Id {
            self.world.id
        }

        fn world(&self) -> World {
            self.world.clone()
        }

        fn port(&self) -> Option<u16> {
            self.port
        }

        fn host(&self) -> String {
            self.host.clone()
        }

        fn hostname(&self) -> Option<String> {
            Some(self.hostname.clone())
        }

        async fn update_world(&mut self, world: World) -> Result<()> {
            self.hostname = world.hostname.clone();
            self.world = world;
            let client = reqwest::Client::new();
            let server = self.server().await;

            match server {
                Ok(server) => {
                    self.port = server.port;
                    Ok(())
                }
                Err(err) => {
                    bail!("Failed to update world: {}", err.to_string());
                }
            }
        }

        async fn config(&self) -> Result<HashMap<String, String>> {
            todo!()
        }

        async fn set_config(&mut self, _config: HashMap<String, String>) -> Result<()> {
            todo!()
        }

        async fn status(&self) -> Result<MinecraftServerStatus, anyhow::Error> {
            let server = self.server().await?;
            Ok(server.status)
        }

        async fn remove(&mut self) -> Result<()> {
            debug!("Requesting minimanager to remvoe server");
            let client = reqwest::Client::new();
            Ok(serde_json::from_str(
                &client
                    .post(format!(
                        "{}api/worlds/remove",
                        CONFIG.remote.host.to_string()
                    ))
                    .header(
                        "Authorization",
                        format!("Bearer {}", crate::config::secrets::SECRETS.api_secret),
                    )
                    .body(serde_json::to_string(&self.world).unwrap())
                    .send()
                    .await?
                    .text()
                    .await?,
            )?)
        }

        async fn refresh(&mut self) -> bool {
            false
        }
    }
}

pub mod util {
    use crate::minecraft::server::SERVERS;

    pub async fn refresh_servers() {
        for (_id, server) in SERVERS.lock().await.iter_mut() {
            server.lock().await.refresh().await;
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ServerConfigLimit {
    MoreThan(i64),
    LessThan(i64),
    Whitelist(Vec<String>),
}

impl Serialize for ServerConfigLimit {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ServerConfigLimit::MoreThan(val) => serializer.serialize_str(&format!(">{val}")),
            ServerConfigLimit::LessThan(val) => serializer.serialize_str(&format!("<{val}")),
            ServerConfigLimit::Whitelist(vals) => serializer.serialize_str(&vals.join("|")),
        }
    }
}

impl<'de> Deserialize<'de> for ServerConfigLimit {
    fn deserialize<D>(deserializer: D) -> result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        if let Some(val) = s.strip_prefix(">") {
            if let Ok(val) = val.parse() {
                return Ok(ServerConfigLimit::MoreThan(val));
            };
        } else if let Some(val) = s.strip_prefix("<") {
            if let Ok(val) = val.parse() {
                return Ok(ServerConfigLimit::LessThan(val));
            };
        };
        Ok(ServerConfigLimit::Whitelist(
            s.split('|').map(String::from).collect(),
        ))
    }
}
