use crate::database::objects::World;
use crate::database::types::Id;
use anyhow::{Context, Result, anyhow};
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, LazyLock, Mutex};

type ServerMutex = Arc<Mutex<Box<dyn MinecraftServer>>>;
static SERVERS: LazyLock<Mutex<HashMap<Id, ServerMutex>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Copy, Clone)]
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

pub fn get_all_worlds() -> Vec<World> {
    SERVERS
        .lock()
        .unwrap()
        .values()
        .map(|server| {
            server
                .lock()
                .expect("failed to lock server")
                .world()
                .clone()
        })
        .collect()
}

impl Serialize for MinecraftServerStatus {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            MinecraftServerStatus::Running => serializer.serialize_str("running"),
            MinecraftServerStatus::Exited(code) => {
                serializer.serialize_str(&format!("exited: {code}"))
            }
        }
    }
}

pub trait MinecraftServer: Send {
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
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn id(&self) -> Id;
    fn status(&self) -> &MinecraftServerStatus;
    fn port(&self) -> Option<u16>;
    fn host(&self) -> String;
    fn world(&self) -> World;
    fn set_world(&mut self, world: World);
    fn update_world(&mut self, world: World) -> Result<()> {
        self.stop()?;
        self.set_world(world);
        self.start()?;
        Ok(())
    }
    fn refresh(&mut self);
    fn read_console(&mut self) -> Result<String>;
    fn write_console(&mut self, data: &[u8]) -> Result<()>;
    fn read_file(&self, path: &str) -> Result<String>;
    fn write_file(&self, path: &str, data: &str) -> Result<()>;
    fn remove_file(&self, path: &str) -> Result<()>;
}

pub mod internal {
    use crate::database::objects::World;
    use crate::database::types::Id;
    use crate::minecraft::server::{MinecraftServer, MinecraftServerStatus};
    use crate::util;
    use anyhow::Result;
    use anyhow::{Context, bail};
    use log::{debug, info, warn};
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
            .find(|&port| !servers.contains(&port))
    }

    pub struct InternalServer {
        status: MinecraftServerStatus,
        world: World,
        directory: PathBuf,
        port: Option<u16>,
        process: Option<Popen>,
    }
    impl InternalServer {
        pub fn new(world: &World) -> Result<Self> {
            Ok(Self {
                status: MinecraftServerStatus::Exited(0),
                world: world.clone(),
                directory: util::dirs::worlds_dir()
                    .join(format!("{}/{}", world.owner_id, world.id)),
                port: None,
                process: None,
            })
        }
    }

    impl MinecraftServer for InternalServer {
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
                bail!("server already running");
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
            let command = Exec::cmd("java")
                .arg("-jar")
                .arg(jar_path.display().to_string())
                .arg("-nogui")
                .cwd(self.directory.clone())
                .stdin(subprocess::Redirection::Pipe)
                .stdout(subprocess::Redirection::Pipe)
                .stderr(subprocess::Redirection::Pipe)
                .popen()
                .inspect_err(|_| {
                    self.status = MinecraftServerStatus::Exited(1);
                })?;

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
        fn id(&self) -> Id {
            self.world.id
        }
        fn status(&self) -> &MinecraftServerStatus {
            &self.status
        }

        fn port(&self) -> Option<u16> {
            self.port
        }

        fn host(&self) -> String {
            "0.0.0.0".to_string()
        }

        fn world(&self) -> World {
            self.world.clone()
        }

        fn set_world(&mut self, world: World) {
            self.world = world;
        }

        fn refresh(&mut self) {
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

    impl Drop for InternalServer {
        fn drop(&mut self) {
            if let Some(mut process) = self.process.take() {
                process.kill().expect("Failed to kill process");
            }
        }
    }
}

pub mod util {
    use crate::minecraft::server::SERVERS;

    pub fn refresh_servers() {
        for (_id, server) in SERVERS.lock().expect("couldn't get servers").iter_mut() {
            server.lock().expect("failed to lock server").refresh()
        }
    }
}
