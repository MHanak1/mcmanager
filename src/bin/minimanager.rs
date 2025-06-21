use anyhow::Result;
use log::info;
use mcmanager::api::filters::with_bearer_token;
use mcmanager::api::util::rejections;
use mcmanager::config::secrets::SECRETS;
use mcmanager::database::objects::World;
use mcmanager::database::types::Id;
use mcmanager::minecraft::server;
use mcmanager::minecraft::server::MinecraftServerStatus;
use mcmanager::minecraft::server::internal::InternalServer;
use mcmanager::{api, config::Config, util::dirs};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::{io::Write, thread};
use warp::http::StatusCode;
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

    let list_worlds = warp::path!("api" / "worlds")
        .and(warp::get())
        .and(with_bearer_token())
        .and_then(|token: String| async move {
            if SECRETS.api_secret.to_string() == token {
                world_list().await
            } else {
                Err(reject::custom(rejections::Unauthorized))
            }
        });

    /*
    let get_world = warp::path!("api" / "worlds")
        .and(warp::get())
        .and(with_bearer_token())
        .and(warp::body::json())
        .and_then(|id: String, token: String, world: World| async move {
            println!("hii");
            if SECRETS.api_secret.to_string() == token {
                warp:: server::get_or_create_server(&world)
            } else {
                Err(reject::custom(rejections::Unauthorized))
            }
        });
     */

    let create_world = warp::path!("api" / "worlds")
        .and(warp::post().or(warp::put()).unify())
        .and(with_bearer_token())
        .and(warp::body::json())
        .and_then(|token: String, world: World| async move {
            if token == SECRETS.api_secret.to_string() {
                create_or_update_server(world).await
            } else {
                Err(reject::custom(rejections::Unauthorized))
            }
        });

    let remove_world = warp::path!("api" / "worlds" / "remove")
        .and(warp::post())
        .and(with_bearer_token())
        .and(warp::body::json())
        .and_then(|token: String, world: World| async move {
            if token == SECRETS.api_secret.to_string() {
                world_remove(world.id).await
            } else {
                Err(reject::custom(rejections::Unauthorized))
            }
        });

    let world_status = warp::path!("api" / "worlds" / "status")
        .and(warp::get())
        .and(with_bearer_token())
        .and(warp::body::json())
        .and_then(|token: String, world: World| async move {
            if SECRETS.api_secret.to_string() == token {
                match server::get_or_create_server(&world).await {
                    Ok(server) => Ok(warp::reply::json(
                        &server.lock().await.status().await.map_err(|err| {
                            warp::reject::custom(rejections::InternalServerError::from(err))
                        })?,
                    )),
                    Err(err) => Err(reject::custom(rejections::InternalServerError::from(err))),
                }
            } else {
                Err(reject::custom(rejections::Unauthorized))
            }
        });

    let log = warp::log("info");

    warp::serve(
        create_world
            .or(remove_world)
            .or(world_status)
            .or(list_worlds)
            .recover(api::handlers::handle_rejection)
            .with(log),
    )
    //TODO: change this back
    .run(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::from_str(&config.listen_address).expect("invalid listen_address")),
        config.remote.port,
    ))
    .await;

    Ok(())
}

async fn world_list() -> std::result::Result<impl Reply, warp::Rejection> {
    Ok(warp::reply::json(&server::get_all_worlds().await))
}

/*
fn world_get(id: Id) -> std::result::Result<impl Reply, warp::Rejection> {
    match server::get_server(id) {
        Some(server) => match server.lock() {
            Ok(server) => Ok(warp::reply::json(&server.world().map_err(|err| {
                warp::reject::custom(rejections::InternalServerError::from(err))
            })?)),
            Err(err) => Err(reject::custom(rejections::InternalServerError::from(err))),
        },
        None => Err(reject::custom(rejections::NotFound)),
    }
}
 */

async fn world_remove(id: Id) -> std::result::Result<impl Reply, warp::Rejection> {
    match server::get_server(id).await {
        Some(server) => {
            match server
                .lock()
                .await
                .status()
                .await
                .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?
            {
                MinecraftServerStatus::Running => Ok(warp::reply::with_status(
                    warp::reply::json(&"server still running"),
                    StatusCode::UNPROCESSABLE_ENTITY,
                )),
                MinecraftServerStatus::Exited(_) => {
                    //TODO: when server is removed it's files should probably be removed as well
                    server::remove_server(&id).await;
                    Ok(warp::reply::with_status(
                        warp::reply::json(&"removed"),
                        StatusCode::OK,
                    ))
                }
            }
        }
        None => Err(reject::custom(rejections::NotFound)),
    }
}

async fn create_or_update_server(world: World) -> std::result::Result<impl Reply, warp::Rejection> {
    let mut server = server::get_or_create_server(&world)
        .await
        .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;
    server
        .lock()
        .await
        .update_world(world.clone())
        .await
        .map_err(|err| warp::reject::custom(rejections::InternalServerError::from(err)))?;
    Ok(warp::reply::with_status(
        warp::reply::json(&world),
        StatusCode::OK,
    ))
}
/*

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
            let server = Box::new(InternalServer::new(world.clone()).map_err(|err| {
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
    Ok(warp::reply::json(&server.world().map_err(|err| {
        warp::reject::custom(rejections::InternalServerError::from(err))
    })?))
}
 */
