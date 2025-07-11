use std::collections::HashMap;
use std::fs;
use crate::config::CONFIG;
use crate::minecraft::server;
use crate::minecraft::server::{MinecraftServerCollection, MinecraftServerStatus};
use crate::util;
use crate::util::dirs;
use log::{error, info, warn};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;
use color_eyre::eyre::{bail, ContextCompat};
use subprocess::{Exec, ExitStatus, Popen};
use crate::config::secrets::SECRETS;

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
       Ok(Self{
           status: MinecraftServerStatus::Exited(0),
           servers,
           path: util::dirs::infrarust_dir(),
           process: None,
           hosts: Default::default(),
       })
    }

    async fn add_server(&mut self, hostname: String, address: String) -> color_eyre::Result<()> {
        self.hosts.insert(hostname.clone(), address.clone());
        let mut file = File::create(self.path.join(format!("proxies/{}.yml", hostname)))?;
        file.write_all(
            include_str!("../resources/default_infrarust_server_config.yml")
                .replace("$hostname", &format!("{}.{}", hostname, CONFIG.proxy.hostname))
                .replace("$address", &format!("{}", address))
                .as_bytes()
        )?;
        Ok(())
    }

    async fn remove_server(&mut self, hostname: String) -> color_eyre::Result<()> {
        self.hosts.remove(&hostname);
        let path = self.path.join(format!("proxies/{}.yml", hostname));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

impl MinecraftProxy for InfrarustServer {
    async fn start(&mut self) -> color_eyre::Result<()> {
        std::fs::create_dir_all(&self.path)?;

        let executable_path = self.path.join(CONFIG.proxy.infrarust_executable_name.clone());
        if !executable_path.exists() {
            bail!(format!("Infrarust executable {} not found", self.path.display()));
        }

        let config_path = self.path.join("config.yaml");
        if !config_path.exists() {
            let mut config_file = File::create(config_path)?;
            config_file.write_all(include_bytes!("../resources/default_infrarust_config.yml"))?;
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
            //try to kill velocity, if it fails, terminate it.
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
                MinecraftServerStatus::Running => { self.status = MinecraftServerStatus::Exited(1); 1 }
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
                    "Infrarust process exited with code {:?}. Restarting it.",
                    exit_code
                );
                self.start().await?;
            } else {
                self.process = Some(process);
            }
        } else {
            self.start().await?;
        }

        let mut new_hosts = HashMap::new();
        for server in self.servers.get_all_servers().await {
            let server = server.lock().await;
            if let Some(port) = server.port() {
                if let Some(hostname) = server.hostname() {
                    let address =  format!("{}:{}", server.host(), port);
                    if self.hosts.get(&hostname) != Some(&address) {
                        self.add_server(hostname.clone(), address.clone()).await?;
                    }
                    new_hosts.insert(hostname, address);
                }
            }
        }

        for (hostname, address) in self.hosts.clone() {
            if !new_hosts.contains_key(hostname.as_str()) {
                self.remove_server(hostname.clone()).await?;
            }
        }


        Ok(())
    }
}

pub struct InternalVelocityServer {
    status: MinecraftServerStatus,
    servers: MinecraftServerCollection,
    path: PathBuf,
    process: Option<Popen>,
    old_hosts: Vec<(String, String)>, //keep the list of hosts to not rewrite the file if not needed
}
impl InternalVelocityServer {
    pub fn new(servers: MinecraftServerCollection) -> color_eyre::Result<Self> {
        Ok(Self {
            status: MinecraftServerStatus::Exited(0),
            servers,
            path: util::dirs::velocity_dir(),
            process: None,
            old_hosts: vec![],
        })
    }

    async fn update_server_list(&mut self, hosts: &[(String, String)]) -> color_eyre::Result<()> {
        info!("Updating the velocity config");

        let mut config = String::new();
        let mut file = File::open(dirs::base_dir().join("velocity_config.toml"))?;
        file.read_to_string(&mut config)?;

        let mut servers_string = String::new();
        let mut hosts_string = String::new();
        for (ip, host) in hosts {
            //println!("{}: {}", ip, host);
            servers_string.push_str(format!("{host} = \"{ip}\"\n").as_str());
            hosts_string.push_str(&format!(
                "\"{host}.{}\" = [\n    \"{host}\"\n]\n",
                CONFIG.proxy.hostname
            ));
        }
        let binding = config.replace("$servers", servers_string.as_str());
        let config = &binding;
        let binding = config.replace("$hosts", hosts_string.as_str());
        let config = &binding;
        let binding = config.replace("$port", CONFIG.proxy.port.to_string().as_str());
        let config = &binding;

        let mut file = File::create(util::dirs::velocity_dir().join("velocity.toml"))?;
        file.write_all(config.as_bytes())?;

        Ok(())
    }
}

impl MinecraftProxy for InternalVelocityServer {
    async fn start(&mut self) -> color_eyre::Result<()> {
        std::fs::create_dir_all(&self.path)?;
        let jar_path = self.path.join(CONFIG.proxy.velocity_executable_name.clone());
        if !jar_path.exists() {
            bail!("{} doesn't exist", jar_path.display());
        }

        if self.process.is_some() {
            bail!("velocity already running");
        }
        let config_path = dirs::base_dir().join("velocity_config.toml");
        if !config_path.exists() {
            let mut file = File::create(&config_path)?;
            file.write_all(include_bytes!("../resources/velocity_config.toml"))?;
        }

        let config_path = dirs::velocity_dir().join("forwarding.secret");
        let mut secret_file = File::create(&config_path)?;
        secret_file.write_all(SECRETS.forwarding_secret.as_bytes())?;

        let command = format!("java -jar {}", jar_path.display());
        //println!("{command}");
        let command = Exec::shell(command)
            .cwd(self.path.clone())
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

    async fn stop(&mut self) -> color_eyre::Result<ExitStatus> {
        if let Some(mut process) = self.process.take() {
            //try to kill velocity, if it fails, terminate it.
            #[allow(clippy::collapsible_if)]
            if process.kill().is_err() {
                if process.terminate().is_err() {
                    bail!("failed to terminate velocity");
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
            warn!("no velocity process to stop");
            let code = match self.status {
                MinecraftServerStatus::Exited(code) => code,
                MinecraftServerStatus::Running => {
                    error!("server status is Running, but the process doesn't exist");
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
                    "Velocity process exited with code {:?}. Restarting it.",
                    exit_code
                );
                self.start().await?;
            } else {
                self.process = Some(process);
            }
        } else {
            self.start().await?;
        }

        let mut hosts = vec![];
        for server in self.servers.get_all_servers().await {
            let server = server.lock().await;
            //println!("{:?}", server.world());
            if let Some(port) = server.port() {
                if let Some(hostname) = server.hostname() {
                    //this pings every server every time the hostname is updated. a better solution should be found for this
                    //if let Ok(MinecraftServerStatus::Running) = server.status().await {
                    hosts.push((format!("{}:{}", server.host(), port), hostname));
                    //}
                }
                //else { println!("{}", "hostname is none"); }
            }
            //else { println!("port is none") }
        }
        if !hosts.eq(&self.old_hosts) {
            self.update_server_list(&hosts).await?;
            self.old_hosts = hosts;

            //self.process.unwrap().communicate(Some("velocity reload\n"))?;
            let mut process = self.process.take().context("Failed to get process")?;
            if let Some(mut stdin) = process.stdin.take() {
                stdin.write_all(b"velocity reload\n")?;
                stdin.flush()?;
                process.stdin = Some(stdin);
            }
            self.process = Some(process);
        }

        Ok(())
    }

    /*
    fn update_if_needed(&mut self) -> color_eyre::Result<()> {
        if self.should_update{
            self.should_update = false;
            self.update()?;
        }
        Ok(())
    }
     */
}
