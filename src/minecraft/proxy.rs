use crate::config::CONFIG;
use crate::minecraft::server::{MinecraftServerCollection, MinecraftServerStatus};
use crate::util;
use color_eyre::eyre::bail;
use log::{error, warn};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{Write};
use std::path::PathBuf;
use std::time::Duration;
use async_trait::async_trait;
use subprocess::{Exec, ExitStatus, Popen};

#[async_trait]
pub trait MinecraftProxy {
    async fn start(&mut self) -> color_eyre::Result<()>;
    async fn stop(&mut self) -> color_eyre::Result<ExitStatus>;
    async fn status(&self) -> MinecraftServerStatus;
    async fn update(&mut self) -> color_eyre::Result<()>;
}

pub struct InfrarustServer {
    status: MinecraftServerStatus,
    servers: MinecraftServerCollection,
    path: PathBuf,
    process: Option<Popen>,
    hosts: HashMap<String, String>,
}

impl InfrarustServer {
    pub fn new(servers: MinecraftServerCollection) -> color_eyre::Result<Self> {
        Ok(Self {
            status: MinecraftServerStatus::Exited(0),
            servers,
            path: util::dirs::infrarust_dir(),
            process: None,
            hosts: HashMap::default(),
        })
    }

    fn add_server(&mut self, hostname: &str, address: &str) -> color_eyre::Result<()> {
        self.hosts.insert(hostname.to_string(), address.to_string());
        let mut file = File::create(self.path.join(format!("proxies/{hostname}.yml")))?;
        file.write_all(
            include_str!("../resources/configs/default_infrarust_server_config.yml")
                .replace(
                    "$hostname",
                    &format!("{}.{}", hostname, CONFIG.proxy.hostname),
                )
                .replace("$address", address)
                .as_bytes(),
        )?;
        Ok(())
    }

    fn remove_server(&mut self, hostname: &str) -> color_eyre::Result<()> {
        self.hosts.remove(hostname);
        let path = self.path.join(format!("proxies/{hostname}.yml"));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[async_trait]
impl MinecraftProxy for InfrarustServer {
    async fn start(&mut self) -> color_eyre::Result<()> {
        //TODO: switch to an embedded infrarust server
        std::fs::create_dir_all(&self.path)?;

        let executable_path = self
            .path
            .join(CONFIG.proxy.infrarust_executable_name.clone());
        if !executable_path.exists() {
            bail!(format!(
                "Infrarust executable {} not found",
                self.path.display()
            ));
        }

        let config_path = self.path.join("config.yaml");
        if !config_path.exists() {
            let mut config_file = File::create(config_path)?;
            config_file.write_all(include_bytes!("../resources/configs/default_infrarust_config.yml"))?;
        }
        fs::create_dir_all(self.path.join("proxies"))?;

        let command = Exec::shell(executable_path)
            .cwd(self.path.clone())
            .stdin(subprocess::Redirection::Pipe)
            .stdout(subprocess::Redirection::Pipe)
            .stderr(subprocess::Redirection::Pipe)
            .popen()
            .inspect_err(|_| {
                self.status = MinecraftServerStatus::Exited(1);
            })
            .expect("Failed to run infrarust");

        self.process = Some(command);

        self.status = MinecraftServerStatus::Running;

        Ok(())
    }

    async fn stop(&mut self) -> color_eyre::Result<ExitStatus> {
        if let Some(mut process) = self.process.take() {
            //try to kill infrarust, if it fails, terminate it.
            #[allow(clippy::collapsible_if)]
            if process.kill().is_err() {
                if process.terminate().is_err() {
                    bail!("failed to terminate infrarust");
                }
            }

            let result = process.wait_timeout(Duration::new(5, 0));
            match result {
                Ok(status) => {
                    match status {
                        Some(status) => {
                            //TODO: fix status code
                            self.status = MinecraftServerStatus::Exited(0);
                            Ok(status)
                        }
                        None => {
                            bail!("Exit status is none");
                        }
                    }
                }
                Err(err) => {
                    bail!("Honestly i don't know when this would occur: {}", err);
                }
            }
        } else {
            warn!("no process to stop");
            let code = match self.status {
                MinecraftServerStatus::Exited(code) => code,
                MinecraftServerStatus::Running => {
                    self.status = MinecraftServerStatus::Exited(1);
                    1
                }
            };
            Ok(ExitStatus::Exited(code))
        }
    }

    async fn status(&self) -> MinecraftServerStatus {
        self.status
    }

    async fn update(&mut self) -> color_eyre::Result<()> {
        if let Some(mut process) = self.process.take() {
            if let Some(exit_code) = process.poll() {
                error!(
                    "Infrarust process exited with code {exit_code:?}. Restarting it."
                );
                self.start().await?;
            } else {
                self.process = Some(process);
            }
        } else {
            self.start().await?;
        }

        let mut new_hosts = HashMap::new();
        for server in self.servers.get_all_servers() {
            let server = server.lock().await;
            if let Some(port) = server.port() {
                if let Some(hostname) = server.hostname() {
                    let address = format!("{}:{}", server.host(), port);
                    if self.hosts.get(&hostname) != Some(&address) {
                        self.add_server(&hostname, &address)?;
                    }
                    new_hosts.insert(hostname, address);
                }
            }
        }

        for (hostname, _) in self.hosts.clone() {
            if !new_hosts.contains_key(hostname.as_str()) {
                self.remove_server(&hostname)?;
            }
        }

        Ok(())
    }
}