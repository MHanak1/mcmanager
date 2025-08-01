use crate::config::{CONFIG, ServerType};
use crate::database::objects::World;
use crate::database::types::Id;
use crate::minecraft;
use color_eyre::Result;
use async_trait::async_trait;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt::Debug;
use std::result;
use std::sync::{Arc, RwLock};
use argon2::password_hash::McfHasher;
use image::DynamicImage;
use tokio::sync::Mutex;
use crate::database::objects::world::MinecraftServerStatusJson;

pub type ServerMutex = Arc<Mutex<Box<dyn MinecraftServer>>>;

#[derive(Debug, Clone)]
pub struct MinecraftServerCollection {
    servers: Arc<RwLock<HashMap<Id, ServerMutex>>>,
}

impl Default for MinecraftServerCollection {
    fn default() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl MinecraftServerCollection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_server(&self, id: Id) -> Option<ServerMutex> {
        let mut server = None;
        {
            // this should hopefully drop SERVERS, right?
            server = self.servers.read().expect("poisoned mutex").get(&id).cloned()
        }
        server
    }

    pub async fn get_or_create_server(
        &self,
        world: &World,
    ) -> Result<minecraft::server::ServerMutex> {
        let mut server = self.get_server(world.id);
        match server {
            Some(server) => Ok(server),
            None => {
                self.add_server(match CONFIG.minecraft_server_type {
                    ServerType::Internal => {
                        Box::new(internal::InternalServer::new(world.clone()).await.map_err(|err| {
                            crate::database::DatabaseError::InternalServerError(err.to_string())
                        })?)
                    }
                    ServerType::Remote => Box::new(external::MinimanagerServer::new(
                        CONFIG
                            .remote
                            .host
                            .host()
                            .expect("invalid remote server hostname")
                            .to_string(),
                        world.clone(),
                    )),
                });
                server = self.get_server(world.id);
                assert!(server.is_some());
                Ok(server.unwrap())
            }
        }
    }

    pub fn add_server(&self, server: Box<dyn MinecraftServer>) {
        self.servers
            .write()
            .expect("poisoned mutex")
            .insert(server.id(), Arc::new(Mutex::new(server)));
    }

    pub fn remove_server(&self, id: &Id) {
        self.servers.write().expect("poisoned mutex").remove(id);
    }

    pub fn get_all_servers(&self) -> Vec<ServerMutex> {
        self.servers.read().expect("poisoned mutex").values().cloned().collect()
    }

    pub async fn get_all_worlds(&self) -> Vec<World> {
        futures::future::join_all(
            self.get_all_servers()
                .into_iter()
                .map(|server: ServerMutex| async move { server.lock().await.world() }),
        )
        .await
    }

    pub async fn poll_servers(&self) {
        futures::future::join_all(
            self.get_all_servers()
                .into_iter()
                .map(|server: ServerMutex| async move { server.lock().await.poll().await }),
        )
        .await;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub world: World,
    pub status: MinecraftServerStatus,
    pub port: Option<u16>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinecraftServerStatus {
    Running,
    Exited(u32),
}

impl From<MinecraftServerStatus> for MinecraftServerStatusJson {
    fn from(value: MinecraftServerStatus) -> MinecraftServerStatusJson {
        match value { 
            MinecraftServerStatus::Running => MinecraftServerStatusJson {
                status: "running".to_string(),
                code: 0,
            },
            MinecraftServerStatus::Exited(code) => MinecraftServerStatusJson {
                status: "exited".to_string(),
                code,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MCStdin {
    Command(String),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McStdout {
    Log{seq: usize, message: String},
    Status(MinecraftServerStatusJson),
}

#[async_trait]
pub trait MinecraftServer: Send + Debug {
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
    async fn set_icon(&mut self, image: DynamicImage) -> Result<()>;
    async fn latest_log(&mut self) -> Result<String>;
    async fn write_console(&mut self, data: String) -> Result<()>;
    async fn status(&self) -> Result<MinecraftServerStatus>;
    /// fully removes the server and its files
    async fn remove(&mut self) -> Result<()>;
    /// updates the status of the server. this should return false if the server is updated through somewhere else
    async fn poll(&mut self) -> bool;
    fn stdout(&self) -> tokio::sync::broadcast::Receiver<McStdout>;
    /*
    async fn ws_handler(
        &self,
        ws: WebSocketUpgrade,
        connect_info: ConnectInfo<SocketAddr>,
    );
     */
}
pub mod internal {
    use std::error::Error;
use crate::config::CONFIG;
    use crate::database::objects::World;
    use crate::database::types::Id;
    use crate::minecraft::server::{MCStdin, McStdout, MinecraftServer, MinecraftServerStatus};
    use crate::util;
    use color_eyre::Result;
    use async_trait::async_trait;
    use log::{debug, error, info, warn};
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::fs::File;
    use std::io::{BufRead, BufReader, BufWriter, Read, Write};
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::sync::{Arc, LazyLock};
    use std::time::Duration;
    use axum::body::Bytes;
    use axum::extract::{ConnectInfo, WebSocketUpgrade};
    use axum::extract::ws::{Message, WebSocket};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use color_eyre::eyre::{bail, ContextCompat};
    use futures::stream::SplitSink;
    use futures::StreamExt;
    use image::{DynamicImage, ImageFormat};
    use image::imageops::FilterType;
    use socketioxide::extract::{Data, SocketRef};
    use socketioxide::SocketIo;
    use subprocess::{Exec, ExitStatus, Popen};
    use tokio::io::AsyncWriteExt;
    use tokio::sync::{RwLock, Mutex, broadcast, mpsc};
    use crate::config::secrets::SECRETS;
    use crate::database::objects::world::MinecraftServerStatusJson;

    pub(crate) static TAKEN_LOCAL_PORTS: LazyLock<std::sync::Mutex<HashSet<u16>>> =
        LazyLock::new(|| std::sync::Mutex::new(HashSet::new()));

    fn get_free_local_port() -> Option<u16> {
        let servers = TAKEN_LOCAL_PORTS.lock().expect("couldn't get servers");
        crate::config::CONFIG
            .world
            .port_range
            .clone()
            .find(|&port| !servers.contains(&port) && port != CONFIG.proxy.port)
    }

    #[derive(Debug)]
    pub struct InternalServer {
        status: MinecraftServerStatus,
        world: World,
        directory: PathBuf,
        port: Option<u16>,
        hostname: String,
        io: Arc<RwLock<InternalSeverIO>>,
        stdin_tx: Option<mpsc::Sender<MCStdin>>,
        stdout_tx: broadcast::Sender<McStdout>,

    }
    #[derive(Default, Debug)]
    pub struct InternalSeverIO {
        process: Option<Arc<Mutex<Popen>>>,
        output_task: Option<tokio::task::JoinHandle<()>>,
        input_task: Option<tokio::task::JoinHandle<()>>,
    }

    impl InternalServer {
        pub async fn new(world: World) -> Result<Self> {
            let enabled = world.enabled;

            let (stdout_tx, _) = broadcast::channel(128);

            let mut new = Self {
                status: MinecraftServerStatus::Exited(0),
                hostname: world.hostname.clone(),
                directory: util::dirs::worlds_dir()
                    .join(format!("{}/{}", world.owner_id, world.id)),
                port: None,
                world,
                io: Default::default(),
                stdin_tx: None,
                stdout_tx,
            };
            if enabled {
                new.start().await?;
            }
            Ok(new)
        }

        fn initialise_files(&self) -> Result<()> {
            debug!("creating dir for server {}", self.world.id);
            fs::create_dir_all(self.directory.clone())?;
            let port = self
                .port()
                .context("Port not set, cannot initialise files")?;

            //todo: maybe remove files that shouldn't be there

            let properties = self
                .read_file("server.properties")
                .unwrap_or_default();
            let mut properties = crate::minecraft::util::parse_minecraft_properties(&properties);
            properties.insert(String::from("query.port"), format!("{port}"));
            properties.insert(String::from("server-port"), format!("{port}",));
            properties.insert(String::from("rcon.port"), format!("{port}",));

            let properties = crate::minecraft::util::create_minecraft_properties(properties);
            debug!("writing server.properties");
            self.write_file("server.properties", &properties)?;

            debug!("writing eula.txt");
            self.write_file("eula.txt", "eula=true")?;

            Ok(())
        }

        async fn start(&mut self) -> Result<()> {
            let jar_path =
                util::dirs::versions_dir().join(format!("{}.jar", self.world.version_id));
            if !jar_path.exists() {
                bail!("{} doesn't exist", jar_path.display());
            }

            if !self.directory.exists() {
                std::fs::create_dir_all(&self.directory)?;
            }

            if self.io.read().await.process.is_some() {
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

            self
                .initialise_files()
                .inspect_err(|_| {
                    TAKEN_LOCAL_PORTS
                        .lock()
                        .expect("failed to lock local ports")
                        .remove(&self.port.unwrap());
                })?;
            debug!("starting server {}", self.id());
            let command = CONFIG.world.java_launch_command.clone();
            let command = command.replace("%jar%", jar_path.display().to_string().as_str());
            let command = command.replace(
                "%min_mem%",
                &format!("-Xms{}m", CONFIG.world.minimum_memory),
            );
            let command = command.replace(
                "%max_mem%",
                &format!("-Xmx{}m", self.world.allocated_memory),
            );
            //println!("{command}");
            let mut command = Exec::shell(command)
                .cwd(self.directory.clone())
                .stdin(subprocess::Redirection::Pipe)
                .stdout(subprocess::Redirection::Pipe)
                .stderr(subprocess::Redirection::Pipe)
                .popen()
                .inspect_err(|_| {
                    self.status = MinecraftServerStatus::Exited(1);
                })
                .expect(":<");

            let (stdin_tx, mut stdin_rx) = mpsc::channel(64);
            self.stdin_tx = Some(stdin_tx);

            let out_task = tokio::task::spawn({
                let stdout_tx = self.stdout_tx.clone();
                let stdout = command.stdout.take().unwrap();
                async move {
                    //output
                    let reader = BufReader::new(stdout);
                    for (seq, line) in reader.lines().enumerate() {
                        let message = line.expect("invalid output line");
                        let _ = stdout_tx.send(McStdout::Log{seq, message});
                    }
                }
            });

            let input_task = tokio::task::spawn_blocking({
                let mut stdin = command.stdin.take().unwrap();
                move || {
                    while let Some(command) = stdin_rx.blocking_recv() {
                        match command {
                            MCStdin::Command(command) => {
                                if let Err(err) = stdin.write_all(command.as_bytes()) {
                                    error!("failed to write command: {err}");
                                }
                            }
                        }
                    }
                }
            });



            self.io.write().await.process = Some(Arc::new(Mutex::new(command)));
            self.io.write().await.output_task = Some(out_task);
            self.io.write().await.input_task = Some(input_task);


            self.status = MinecraftServerStatus::Running;
            _ = self.stdout_tx.send(McStdout::Status(MinecraftServerStatusJson::from(self.status)));

            Ok(())
        }

        async fn stop(&mut self) -> Result<()> {
            let stop_result = self.write_console(String::from("stop\n")).await;
            let process = if let Some(process) = self.io.read().await.process.clone() {
                process
            } else {
                debug!("not stopping process as it is not running");
                return Ok(());
            };

            if stop_result.is_err() {
                let _ = process.lock().await.kill();
            }

            let result = process.lock().await.wait_timeout(Duration::from_secs(
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
                }
                None => {
                    process.lock().await.terminate()?;
                    warn!("stopped server {} after timeout period elapsed", self.id());
                    self.status = MinecraftServerStatus::Exited(1);
                }
            }
            let _ = self.stdout_tx.send(McStdout::Status(MinecraftServerStatusJson::from(self.status)));

            Ok(())
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


        fn read_file(&self, path: &str) -> Result<String> {
            let mut output = String::new();
            let mut file = File::open(self.directory.join(path))?;
            file.read_to_string(&mut output)?;
            Ok(output)
        }

        fn write_file(&self, path: &str, data: &str) -> Result<()> {
            let path = self.directory.join(path);
            std::fs::create_dir_all(path.parent().unwrap_or(self.directory.as_path()))?;
            let mut file = File::create(path)?;
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
                .unwrap_or_default();
            Ok(crate::minecraft::util::parse_minecraft_properties(
                &properties,
            ))
        }

        async fn set_config(&mut self, config: HashMap<String, String>) -> Result<()> {
            let properties = crate::minecraft::util::create_minecraft_properties(config);
            self.write_file("server.properties", &properties)?;
            Ok(())
        }

        async fn set_icon(&mut self, image: DynamicImage) -> Result<()> {
            let image = image.resize(64, 64, FilterType::CatmullRom);


            let image_file =
                std::fs::File::create(&self.directory.join("server-icon.png"))?;
            let mut writer = BufWriter::new(image_file);
            image.write_to(&mut writer, ImageFormat::Png)?;

            Ok(())
        }

        async fn latest_log(&mut self) -> Result<String> {
            self.read_file("logs/latest.log")
        }

        async fn write_console(&mut self, data: String) -> Result<()> {
            if let Some(stdin_tx) = &self.stdin_tx {
                stdin_tx.send(MCStdin::Command(data)).await?;
                Ok(())
            } else {
                match self.status {
                    MinecraftServerStatus::Exited(_) => {}
                    _ => {
                        self.status = MinecraftServerStatus::Exited(1);
                    }
                }
                bail!("Cannot write to console: server is not running");
            }
            /*
            match self.io.write().await.process {
                Some(ref mut process) => match process.lock().await.stdin {
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
             */
        }

        async fn status(&self) -> Result<MinecraftServerStatus, color_eyre::eyre::Error> {
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

        async fn poll(&mut self) -> bool {
            let exit_status = if let Some(process) = &self.io.read().await.process.clone() {
                process.lock().await.exit_status()
            } else { None };
            if let Some(exit_status) = exit_status {
                self.io.write().await.process = None;

                match exit_status {
                    ExitStatus::Exited(code) => {
                        self.status = MinecraftServerStatus::Exited(code);
                    }
                    _ => {
                        self.status = MinecraftServerStatus::Exited(1);
                    }
                }
                _ = self.stdout_tx.send(McStdout::Status(MinecraftServerStatusJson::from(self.status)));
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
            true
        }

        fn stdout(&self) -> broadcast::Receiver<McStdout> {
            self.stdout_tx.subscribe()
        }

    }

    /*
    impl Drop for InternalServer {
        fn drop(&mut self) {
            if let Some(mut process) = self.io.write().process.clone() {
                process.blocking_lock().kill().expect("Failed to kill process");
            }
        }
    }
     */
}

pub mod external {
    use crate::config::CONFIG;
    use crate::database::objects::World;
    use crate::database::types::Id;
    use crate::minecraft::server::{MCStdin, McStdout, MinecraftServer, MinecraftServerStatus, Server};
    use color_eyre::{Result};
    use async_trait::async_trait;
    use log::debug;
    use reqwest::StatusCode;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use axum::extract::ws::WebSocket;
    use color_eyre::eyre::bail;
    use image::DynamicImage;
    use socketioxide::extract::SocketRef;
    use tokio::sync::broadcast::Receiver;
    use tokio::sync::mpsc::Sender;

    #[derive(Debug)]
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

        async fn set_icon(&mut self, image: DynamicImage) -> Result<()> {
            todo!()
        }

        async fn latest_log(&mut self) -> Result<String> {
            todo!()
        }

        async fn write_console(&mut self, data: String) -> Result<()> {
            todo!()
        }

        async fn status(&self) -> Result<MinecraftServerStatus, color_eyre::eyre::Error> {
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

        async fn poll(&mut self) -> bool {
            false
        }

        fn stdout(&self) -> Receiver<McStdout> {
            todo!()
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
