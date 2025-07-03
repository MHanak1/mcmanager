use crate::config::CONFIG;
use crate::minecraft::server;
use crate::minecraft::server::MinecraftServerStatus;
use crate::util;
use crate::util::dirs;
use anyhow::{Context, bail};
use log::{error, info, warn};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;
use subprocess::{Exec, ExitStatus, Popen};

pub trait VelocityServer {
    async fn start(&mut self) -> anyhow::Result<()>;
    async fn stop(&mut self) -> anyhow::Result<ExitStatus>;
    async fn status(&self) -> MinecraftServerStatus;
    async fn update_server_list(hosts: &[(String, String)]) -> anyhow::Result<()>;
    async fn update(&mut self) -> anyhow::Result<()>;
    //fn update_if_needed(&mut self) -> anyhow::Result<()>;
    async fn make_should_update(&mut self);
}

pub struct InternalVelocityServer {
    status: MinecraftServerStatus,
    should_update: bool,
    path: PathBuf,
    process: Option<Popen>,
    old_hosts: Vec<(String, String)>, //keep the list of hosts to not rewrite the file if not needed
}
impl InternalVelocityServer {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            status: MinecraftServerStatus::Exited(0),
            should_update: false,
            path: util::dirs::velocity_dir(),
            process: None,
            old_hosts: vec![],
        })
    }
}

impl VelocityServer for InternalVelocityServer {
    async fn start(&mut self) -> anyhow::Result<()> {
        let jar_path = self.path.join(CONFIG.velocity.executable_name.clone());
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

    async fn stop(&mut self) -> anyhow::Result<ExitStatus> {
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

    async fn update_server_list(hosts: &[(String, String)]) -> anyhow::Result<()> {
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
                CONFIG.velocity.hostname.split(":").last().unwrap()
            ));
        }
        let binding = config.replace("$servers", servers_string.as_str());
        let config = &binding;
        let binding = config.replace("$hosts", hosts_string.as_str());
        let config = &binding;

        let mut file = File::create(util::dirs::velocity_dir().join("velocity.toml"))?;
        file.write_all(config.as_bytes())?;

        Ok(())
    }
    async fn update(&mut self) -> anyhow::Result<()> {
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
        for server in server::get_all_servers().await {
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
            self.old_hosts = hosts;
            Self::update_server_list(&self.old_hosts).await?;

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
    fn update_if_needed(&mut self) -> anyhow::Result<()> {
        if self.should_update{
            self.should_update = false;
            self.update()?;
        }
        Ok(())
    }
     */

    async fn make_should_update(&mut self) {
        self.should_update = true;
    }
}
