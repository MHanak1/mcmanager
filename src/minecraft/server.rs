use crate::database::objects::World;
use crate::database::types::Id;
use anyhow::{Context, Result, anyhow};
use log::error;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, LazyLock, Mutex};

type ServerMutex = Arc<Mutex<Box<dyn MinecraftServer>>>;
static SERVERS: LazyLock<Mutex<HashMap<Id, ServerMutex>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub world: World,
    pub status: MinecraftServerStatus,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum MinecraftServerStatus {
    Running,
    Exited(u32),
}

pub fn get_server(id: Id) -> Option<ServerMutex> {
    match SERVERS.lock() {
        Ok(servers) => servers.get(&id).cloned(),
        Err(_) => None,
    }
}

pub fn add_server(server: Box<dyn MinecraftServer>) -> anyhow::Result<()> {
    match SERVERS.lock() {
        Ok(mut servers) => {
            servers.insert(server.id(), Arc::new(Mutex::new(server)));
            Ok(())
        }
        Err(err) => Err(anyhow!("failed to lock servers: {}", err)),
    }
}

pub fn remove_server(id: &Id) -> anyhow::Result<()> {
    match SERVERS.lock() {
        Ok(mut servers) => {
            servers.remove(id);
            Ok(())
        }
        Err(err) => Err(anyhow!("failed to lock servers: {}", err)),
    }
}

pub fn get_all_servers() -> Vec<ServerMutex> {
    SERVERS
        .lock()
        .unwrap()
        .values()
        .map(|server| server.clone())
        .collect()
}

pub fn get_all_worlds() -> Vec<World> {
    SERVERS
        .lock()
        .unwrap()
        .values()
        .filter_map(|server| server.lock().expect("failed to lock server").world().ok())
        .collect()
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

pub trait MinecraftServer: Send {
    fn update_world(&mut self, world: World) -> Result<()>;
    fn id(&self) -> Id;
    fn world(&self) -> Result<World>;
    fn status(&self) -> Result<MinecraftServerStatus>;
    /// to which port is the server bound to
    fn port(&self) -> Option<u16>;
    /// under what subdomain is the server avaliable)
    fn host(&self) -> String;
    /// Where the minecraft server resides
    fn hostname(&self) -> Option<String>;
    /// updates the status of the server. this should return false if the server is updated through somewhere else
    fn refresh(&mut self) -> bool;
}

pub mod internal {
    use crate::config::CONFIG;
    use crate::database::objects::World;
    use crate::database::types::Id;
    use crate::minecraft::server::{MinecraftServer, MinecraftServerStatus, Server};
    use crate::util;
    use anyhow::Result;
    use anyhow::{Context, bail};
    use log::{debug, error, info, warn};
    use std::collections::HashSet;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};
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
        pub fn new(world: &World) -> Result<Self> {
            let mut new = Self {
                status: MinecraftServerStatus::Exited(0),
                world: world.clone(),
                directory: util::dirs::worlds_dir()
                    .join(format!("{}/{}", world.owner_id, world.id)),
                port: None,
                hostname: world.hostname.clone(),
                process: None,
            };
            if world.enabled {
                new.start()?;
            }
            Ok(new)
        }

        fn initialise_files(&self) -> Result<()> {
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
        fn start(&mut self) -> Result<()> {
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

            self.initialise_files()?;
            debug!("starting server {}", self.id());
            let command = CONFIG.world.java_launch_command.clone();
            let command = command.replace("%jar%", jar_path.display().to_string().as_str());
            let command = command.replace(
                "%max_mem%",
                &format!("-Xmx{}m", self.world.allocated_memory),
            );
            println!("{command}");
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

        fn stop(&mut self) -> Result<()> {
            let stop_result = self.write_console(b"stop\n");
            if let Some(mut process) = self.process.take() {
                if stop_result.is_err() {
                    let _ = process.kill();
                }

                let result = process.wait_timeout(crate::config::CONFIG.world.stop_timeout)?;
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

        fn remove_file(&self, path: &str) -> Result<()> {
            std::fs::remove_file(self.directory.join(path))?;
            Ok(())
        }
    }

    impl MinecraftServer for InternalServer {
        fn update_world(&mut self, world: World) -> Result<()> {
            let old = self.world()?;
            if old.allocated_memory != world.allocated_memory || old.version_id != world.version_id
            {
                self.stop()?;
            }

            let enabled = world.enabled;

            self.hostname = world.hostname.clone();
            self.world = world;

            if enabled {
                self.start()?;
            } else {
                self.stop()?;
            }
            Ok(())
        }

        fn id(&self) -> Id {
            self.world.id
        }

        fn world(&self) -> Result<World, anyhow::Error> {
            Ok(self.world.clone())
        }

        fn status(&self) -> Result<MinecraftServerStatus, anyhow::Error> {
            Ok(self.status)
        }

        fn port(&self) -> Option<u16> {
            self.port
        }

        fn host(&self) -> String {
            "localhost".to_string()
        }

        fn hostname(&self) -> Option<String> {
            Some(self.hostname.clone())
        }

        fn refresh(&mut self) -> bool {
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

            /*
            match self.status {
                MinecraftServerStatus::Exited(code) => {
                    if self.world.enabled {
                        match self.start() {
                            Err(err) => {
                                error!("error updating a server: {}", err);
                            }
                            _ => {}
                        }
                    }
                }
                MinecraftServerStatus::Running => {
                    if !self.world.enabled {
                        match self.stop() {
                            Err(err) => {
                                error!("error updating a server: {}", err);
                            }
                            _ => {}
                        }
                    }
                }
            }
             */

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
    use crate::database::objects::World;
    use crate::database::types::Id;
    use crate::minecraft::server::{MinecraftServer, MinecraftServerStatus, Server};
    use anyhow::Result;
    use log::error;
    use serde::Serialize;

    pub struct MinimanagerServer {
        host: String,
        port: u16,
        hostname: String,
        world: World,
    }

    impl MinimanagerServer {
        pub fn new(host: String, port: u16, world: World) -> Self {
            Self {
                host,
                port,
                hostname: world.hostname.clone(),
                world,
            }
        }

        fn world_and_status(&self) -> Result<Server> {
            let client = reqwest::blocking::Client::new();
            let response: Server = serde_json::from_str(
                &client
                    .get(format!("{}:{}/{}", self.host, self.port, self.id()))
                    .body("{\"enabled\": true}")
                    .send()?
                    .text()?,
            )?;
            Ok(response)
        }
    }

    impl MinecraftServer for MinimanagerServer {
        fn update_world(&mut self, world: World) -> Result<()> {
            self.hostname = world.hostname.clone();
            self.world = world;
            let client = reqwest::blocking::Client::new();
            /*let response: Server = serde_json::from_str(
                &client
                    .get(format!("{}:{}/{}", self.host, self.port, self.id()))
                    .body(serde_json::to_string(&self.world).unwrap())
                    .send()?
                    .text()?,
            )?;
             */
            let _ =  &client
                .get(format!("{}:{}/{}", self.host, self.port, self.id()))
                .body(serde_json::to_string(&self.world).unwrap())
                .send()?;

            Ok(())
        }

        fn id(&self) -> Id {
            self.world.id
        }

        fn world(&self) -> Result<World, anyhow::Error> {
            match self.world_and_status() {
                Ok(server) => Ok(server.world),
                Err(err) => {
                    error!("failed to get server status: {err}");
                    Ok(self.world.clone())
                }
            }
        }

        fn status(&self) -> Result<MinecraftServerStatus, anyhow::Error> {
            match self.world_and_status() {
                Ok(server) => Ok(server.status),
                Err(err) => {
                    error!("failed to get server status: {err}");
                    Ok(MinecraftServerStatus::Exited(1))
                }
            }
        }

        fn port(&self) -> Option<u16> {
            Some(self.port)
        }

        fn host(&self) -> String {
            self.host.clone()
        }

        fn hostname(&self) -> Option<String> {
            Some(self.hostname.clone())
        }

        fn refresh(&mut self) -> bool {
            false
        }
    }
}

pub mod util {
    use crate::minecraft::server::SERVERS;

    pub fn refresh_servers() {
        for (_id, server) in SERVERS.lock().expect("couldn't get servers").iter_mut() {
            server.lock().expect("failed to lock server").refresh();
        }
    }
}
