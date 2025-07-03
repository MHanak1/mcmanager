use crate::config::secrets::SECRETS;
use crate::database::DatabaseError;
use crate::database::objects::World;
use crate::database::types::Id;
use crate::minecraft::server;
use crate::minecraft::server::MinecraftServerStatus;
use crate::minecraft::server::internal::InternalServer;
use crate::{api, config::Config, util::dirs};
use anyhow::Result;
use axum::http::StatusCode;
use log::info;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::{io::Write, thread};

#[tokio::main]
async fn main() -> Result<()> {
    Ok(())
    /*
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
                crate::minecraft::server::util::refresh_servers();
                thread::sleep(std::time::Duration::from_millis(1000));
            }
        });

        run(crate::config::CONFIG.clone()).await
    }

    async fn run(config: Config) -> Result<()> {
        info!("Starting minimanager...");

        let list_worlds = axum::path!("api" / "worlds")
            .and(axum::get())
            .and(with_bearer_token())
            .and_then(|token| async move {
                if SECRETS.api_secret == token {
                    world_list().await
                } else {
                    Err(reject::custom(rejections::Unauthorized))
                }
            });

        /*
        let get_world = axum::path!("api" / "worlds")
            .and(axum::get())
            .and(with_bearer_token())
            .and(axum::body::json())
            .and_then(|id: String, token: String, world: World| async move {
                println!("hii");
                if SECRETS.api_secret.to_string() == token {
                    axum:: server::get_or_create_server(&world)
                } else {
                    Err(reject::custom(rejections::Unauthorized))
                }
            });
         */

        let create_world = axum::path!("api" / "worlds")
            .and(axum::post().or(axum::put()).unify())
            .and(with_bearer_token())
            .and(axum::body::json())
            .and_then(|token, world: World| async move {
                if token == SECRETS.api_secret {
                    create_or_update_server(world).await
                } else {
                    Err(reject::custom(rejections::Unauthorized))
                }
            });

        let remove_world = axum::path!("api" / "worlds" / "remove")
            .and(axum::post())
            .and(with_bearer_token())
            .and(axum::body::json())
            .and_then(|token, world: World| async move {
                if token == SECRETS.api_secret {
                    world_remove(world).await
                } else {
                    Err(reject::custom(rejections::Unauthorized))
                }
            });

        let world_status = axum::path!("api" / "worlds" / "status")
            .and(axum::get())
            .and(with_bearer_token())
            .and(axum::body::json())
            .and_then(|token, world: World| async move {
                if SECRETS.api_secret == token {
                    match server::get_or_create_server(&world).await {
                        Ok(server) => Ok(axum::response::json(
                            &server.lock().await.status().await.map_err(|err| {
                                axum::reject::custom(rejections::InternalServerError::from(err))
                            })?,
                        )),
                        Err(err) => Err(reject::custom(rejections::InternalServerError::from(err))),
                    }
                } else {
                    Err(reject::custom(rejections::Unauthorized))
                }
            });

        let log = axum::log("info");

        axum::serve(
            create_world
                .or(remove_world)
                .or(world_status)
                .or(list_worlds)
                .recover(api::handlers::handle_rejection)
                .with(log),
        )
        .run(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::from_str(&config.listen_address).expect("invalid listen_address")),
            config.listen_port,
        ))
        .await;

        Ok(())
         */
}

/*
async fn world_list() -> std::result::Result<impl Reply, axum::Rejection> {
    Ok(axum::response::json(&server::get_all_worlds().await))
}

/*
fn world_get(id: Id) -> std::result::Result<impl Reply, axum::Rejection> {
    match server::get_server(id) {
        Some(server) => match server.lock() {
            Ok(server) => Ok(axum::response::json(&server.world().map_err(|err| {
                axum::reject::custom(rejections::InternalServerError::from(err))
            })?)),
            Err(err) => Err(reject::custom(rejections::InternalServerError::from(err))),
        },
        None => Err(reject::custom(rejections::NotFound)),
    }
}
 */

async fn world_remove(world: World) -> std::result::Result<impl Reply, axum::Rejection> {
    let response = match server::get_or_create_server(&world).await {
        Ok(server) => {
            let mut server = server.lock().await;
            server
                .remove()
                .await
                .map_err(|err| axum::reject::custom(rejections::InternalServerError::from(err)))?;
            Ok(axum::response())
        }
        Err(err) => Err(axum::reject::custom(rejections::InternalServerError::from(
            err,
        ))),
    };
    server::remove_server(&world.id).await;

    response
}

async fn create_or_update_server(world: World) -> std::result::Result<impl Reply, axum::Rejection> {
    let mut server = server::get_or_create_server(&world)
        .await
        .map_err(|err| axum::reject::custom(rejections::InternalServerError::from(err)))?;
    server
        .lock()
        .await
        .update_world(world.clone())
        .await
        .map_err(|err| axum::reject::custom(rejections::InternalServerError::from(err)))?;
    Ok(axum::response::with_status(
        axum::response::json(&world),
        StatusCode::OK,
    ))
}
/*

fn create_or_update_server(world: World) -> std::result::Result<impl Reply, axum::Rejection> {
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
                axum::reject::custom(rejections::InternalServerError::from(err.to_string()))
            })?;

            match server::get_server(world.id) {
                Some(server) => server,
                None => Err(axum::reject::custom(rejections::InternalServerError::from(
                    String::from("can't find the created server"),
                )))?,
            }
        }
    };

    let server = server.lock().expect("failed to lock server");

    println!("Created server {:?}", server.world());
    Ok(axum::response::json(&server.world().map_err(|err| {
        axum::reject::custom(rejections::InternalServerError::from(err))
    })?))
}
 */

 */
