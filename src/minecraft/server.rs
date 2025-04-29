use crate::database::Database;
use crate::database::objects::{Version, World};
use crate::database::types::Id;
use crate::{database, util};
use anyhow::{anyhow, bail, Context};
use log::{debug, error, info, warn};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use subprocess::{Exec, ExitStatus, Popen};
use anyhow::Result;
use std::sync::{LazyLock, Mutex};
use std::collections::{HashMap, HashSet};
use std::thread;

pub(crate) static SERVERS: LazyLock<Mutex<Vec<Box<dyn MinecraftServer>>>> =
    LazyLock::new(|| Mutex::new(vec![]));

pub(crate) static TAKEN_LOCAL_PORTS: LazyLock<Mutex<HashSet<u16>>> =
LazyLock::new(|| Mutex::new(HashSet::new()));
fn get_free_local_port() -> Option<u16> {
    let servers = TAKEN_LOCAL_PORTS.lock().expect("couldn't get servers");
    for port in crate::config::CONFIG.world.port_range.clone() {
        if !servers.contains(&port) {
            return Some(port);
        }
    }
    None
}

pub fn refresh_local_servers() {
    for server in SERVERS.lock().expect("couldn't get servers").iter_mut() {
        server.refresh()
    }
}

pub enum MinecraftServerStatus {
    Running,
    Stopping,
    Exited(u32),
}

pub trait MinecraftServer: Send {
    fn new(world: &World, database: &Database) -> Result<Self, >
    where
        Self: Sized;
    fn start(&mut self, database: &Database) -> Result<(), >;
    fn stop(&mut self, database: &Database) -> Result<(), >;
    fn id(&self) -> Id;
    fn status(&self) -> &MinecraftServerStatus;
    fn refresh(&mut self);
    fn read_console (&mut self) -> Result<String, >;
    fn write_console (&mut self, data: &[u8]) -> Result<(), >;
}

pub struct InternalServer {
    status: MinecraftServerStatus,
    world: World,
    directory: PathBuf,
    port: Option<u16>,
    process: Option<Popen>,
}

impl InternalServer {
    fn initialise_files(&self) -> Result<()> {
        let port = self.port.context("Port not set, cannot initialise files")?;

        if !self.directory.exists() {
            std::fs::create_dir_all(&self.directory)?;
        }
        let mut properties = String::new();
        if let Ok(mut properties_file) = File::open(&self.directory.join("server.properties")) {
            properties_file.read_to_string(&mut properties)?;
        }else {
            properties = include_str!("../resources/server.properties").to_string();
        };
        let mut properties = crate::minecraft::util::parse_minecraft_properties(&properties);
        properties.insert(String::from("query.port"), format!("{}", port));
        properties.insert(String::from("server-port"), format!("{}", port));

        let properties = crate::minecraft::util::create_minecraft_properties(properties);
        let mut properties_file = File::create(self.directory.join("server.properties")).expect("failed to create file");

        properties_file.write_all(properties.as_bytes()).expect("failed to write file");

        let mut eula_file = File::create(self.directory.join("eula.txt"))?;
        eula_file.write_all(b"eula=true")?;

        Ok(())
    }
}

impl MinecraftServer for InternalServer {
    fn new(world: &World, database: &Database) -> Result<Self, > {
        let directory = util::dirs::worlds_dir().join(format!(
            "{}/{}",
            world.owner_id.to_string(),
            world.id.to_string()
        ));

        let mut new = Self {
            status: MinecraftServerStatus::Exited(0),
            world: world.clone(),
            directory,
            port: None,
            process: None,
        };

        Ok(new)
    }

    fn start(&mut self, database: &Database) -> Result<()> {
        let jar_path = util::dirs::versions_dir().join(format!("{}.jar", self.world.version_id.to_string()));
        if !jar_path.exists() {
            bail!("{} doesn't exist", jar_path.display());
        }

        if self.process.is_some() {
            bail!("server already running");
        }

        let port = get_free_local_port().context("No free ports left")?;
        info!("assigning port {} for {}", port, self.world.id);
        self.port = Some(port);
        TAKEN_LOCAL_PORTS.lock().expect("failed to lock local ports").insert(port);

        self.initialise_files()?;
        debug!("starting server {}", self.id());
        let mut command = Exec::cmd("java")
            .arg("-jar")
            .arg(jar_path.display().to_string())
            //.arg("-Djava.security.manager")
            //.arg(format!("-Djava.security.policy={}/{}.policy", util::dirs::policies_dir().display(), self.world.id))
            //.arg("-nogui")
            .cwd(self.directory.clone())
            .stdin(subprocess::Redirection::Pipe)
            .stdout(subprocess::Redirection::Pipe)
            .stderr(subprocess::Redirection::Pipe)
            .popen()
            .map_err(|err| {
                self.status = MinecraftServerStatus::Exited(1);
                err
            })?;

        self.process = Some(command);

        self.status = MinecraftServerStatus::Running;

        Ok(())
    }


    fn stop(&mut self, database: &Database) -> Result<(), > {
        self.status = MinecraftServerStatus::Stopping;
        if let Some(mut process) = self.process.take() {
            self.write_console(b"stop\n").map_err(|err| {process.kill(); err})?;
            let result = process
                .wait_timeout(crate::config::CONFIG.world.stop_timeout)?;
            TAKEN_LOCAL_PORTS.lock().expect("failed to lock local ports").remove(&self.port.unwrap_or(0));
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
                },
                None => {
                    process.kill()?;
                    warn!("stopped server {} after timeout period elapsed", self.id());
                    self.status = MinecraftServerStatus::Exited(1);
                    Ok(())
                }
            }
        } else {
            debug!(
                "Not stopping {}, because it's not running",
                self.world.id
            );
            Ok(())
        }
    }
    fn id(&self) -> Id {
        self.world.id
    }
    fn status(&self) -> &MinecraftServerStatus {
        &self.status
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
                info!("freed the port {} of {} because the server running on it has exited", self.port.unwrap_or(0), self.world.id);
                TAKEN_LOCAL_PORTS.lock().expect("failed to lock local ports").remove(&self.port.unwrap_or(0));
            }
        }
    }

    fn read_console (&mut self) -> Result<String> {
        let mut output = String::new();

        match self.process {
            Some(ref mut process) => {
                match process.stdout {
                    Some(ref mut stdout) => {
                        stdout.read_to_string(&mut output)?;
                    }
                    None => {
                        anyhow::bail!("Could not get process stdout");
                    }
                }
            }
            None => {
                anyhow::bail!("Cannot read console: server is not running");
            }
        }
        Ok(output)
    }

    fn write_console (&mut self, data: &[u8]) -> Result<()> {
        match self.process {
            Some(ref mut process) => {
                match process.stdin {
                    Some(ref mut stdin) => {
                        stdin.write(data)?;
                    }
                    None => {
                        anyhow::bail!("Could not get process stdin");
                    }
                }
            }
            None => {
                anyhow::bail!("Cannot write to console: server is not running");
            }
        }
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