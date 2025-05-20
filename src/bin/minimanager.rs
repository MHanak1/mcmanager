use crate::secrets::SECRETS;
use anyhow::Result;
use log::info;
use mcmanager::api::filters::with_bearer_token;
use mcmanager::api::util::rejections;
use mcmanager::database::objects::World;
use mcmanager::database::types::Id;
use mcmanager::minecraft::server;
use mcmanager::minecraft::server::internal::InternalServer;
use mcmanager::{api, config::Config, util::dirs};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::{io::Write, thread};
use warp::{Filter, Reply, reject};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let config_path = dirs::base_dir().join("config.toml");
    if !config_path.exists() {
        let mut config_file = std::fs::File::create(config_path)?;
        config_file.write_all(include_bytes!("../resources/default_config.toml"))?;
    }
    let secrets_path = dirs::base_dir().join("secrets.toml");
    if !secrets_path.exists() {
        panic!(
            "secrets file missing (it needs to be mounted at {})",
            secrets_path.display()
        )
    }

    thread::spawn(|| {
        loop {
            mcmanager::minecraft::server::util::refresh_servers();
            thread::sleep(std::time::Duration::from_millis(1000));
        }
    });

    run(mcmanager::config::CONFIG.clone()).await
}

async fn run(config: Config) -> Result<()> {
    info!("Starting minimanager...");

    let list_worlds = warp::path!()
        .and(warp::get())
        .and(with_bearer_token())
        .and_then(|token: String| async move {
            if SECRETS.api_secret.to_string() == token {
                world_list()
            } else {
                Err(reject::custom(rejections::Unauthorized))
            }
        });

    let get_world = warp::path!(String)
        .and(warp::get())
        .and(with_bearer_token())
        .and_then(|id: String, token: String| async move {
            let id = Id::from_string(&id);
            if id.is_err() {
                return Err(warp::reject::custom(rejections::BadRequest));
            }
            #[allow(clippy::unwrap_used)]
            let id = id.unwrap();

            if SECRETS.api_secret.to_string() == token {
                world_get(id)
            } else {
                Err(reject::custom(rejections::Unauthorized))
            }
        });

    let create_world = warp::path!()
        .and(warp::post())
        .and(with_bearer_token())
        .and(warp::body::json())
        .and_then(|token: String, world: World| async move {
            if token == SECRETS.api_secret.to_string() {
                create_or_update_server(world)
            } else {
                Err(reject::custom(rejections::Unauthorized))
            }
        });

    let log = warp::log("info");

    warp::serve(
        list_worlds
            .or(get_world)
            .or(create_world)
            .recover(api::handlers::handle_rejection)
            .with(log),
    )
    //TODO: change this back
    .run(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::from_str(&config.listen_address).expect("invalid listen_address")),
        //config.listen_port as u16,
        3031,
    ))
    .await;

    Ok(())
}

fn world_list() -> std::result::Result<impl Reply, warp::Rejection> {
    Ok(warp::reply::json(&server::get_all_worlds()))
}

fn world_get(id: Id) -> std::result::Result<impl Reply, warp::Rejection> {
    match server::get_server(id) {
        Some(server) => match server.lock() {
            Ok(server) => Ok(warp::reply::json(&server.world())),
            Err(err) => Err(warp::reject::custom(rejections::InternalServerError::from(
                err.to_string(),
            ))),
        },
        None => Err(reject::custom(rejections::NotFound)),
    }
}

fn create_or_update_server(world: World) -> std::result::Result<impl Reply, warp::Rejection> {
    let server = match server::get_server(world.id) {
        Some(server) => match server
            .lock()
            .expect("failed to lock server")
            .update_world(world)
        {
            Ok(_) => server.clone(),
            Err(err) => {
                return Err(reject::custom(rejections::InternalServerError::from(
                    err.to_string(),
                )));
            }
        },
        None => {
            let server = Box::new(InternalServer::new(&world).map_err(|err| {
                reject::custom(rejections::InternalServerError::from(err.to_string()))
            })?);
            server::add_server(server).map_err(|err| {
                warp::reject::custom(rejections::InternalServerError::from(err.to_string()))
            })?;

            match server::get_server(world.id) {
                Some(server) => server,
                None => Err(warp::reject::custom(rejections::InternalServerError::from(
                    String::from("can't find the created server"),
                )))?,
            }
        }
    };

    let server = server.lock().expect("failed to lock server");

    println!("Created server {:?}", server.world());
    Ok(warp::reply::json(&server.world()))
}

mod secrets {
    use config::Config;
    use mcmanager::database::types::Token;
    use mcmanager::util;
    use once_cell::sync::Lazy;

    pub struct Secrets {
        pub api_secret: Token,
    }

    impl TryFrom<Config> for Secrets {
        type Error = anyhow::Error;

        fn try_from(config: Config) -> Result<Self, Self::Error> {
            Ok(Self {
                api_secret: config.get("api_secret")?,
            })
        }
    }

    pub static SECRETS: Lazy<Secrets> = Lazy::new(|| {
        Secrets::try_from(
            Config::builder()
                .add_source(config::File::with_name(
                    util::dirs::base_dir()
                        .join("secrets.toml")
                        .display()
                        .to_string()
                        .as_str(),
                ))
                .build()
                .expect("failed to parse secrets"),
        )
        .expect("failed to parse secrets")
    });
}
