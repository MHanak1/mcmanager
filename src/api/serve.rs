use crate::api::filters;
use crate::api::handlers::ApiObject;
use crate::config;
use crate::config::CONFIG;
use crate::database::Database;
use crate::database::objects::{Group, InviteLink, Mod, ModLoader, Session, User, Version, World};
use crate::minecraft::server::{MinecraftServerCollection, Server};
use crate::minecraft::proxy::{InternalVelocityServer, MinecraftProxy};
use crate::util::dirs::icons_dir;
use crate::{api, util};
use axum::extract::{MatchedPath, Path, State};
use axum::http::Request;
use axum::response::Response;
use axum::routing::{MethodRouter, delete, get, post};
use axum::{Router, ServiceExt};
use log::{debug, error, info};
use reqwest::StatusCode;
use sqlx::Encode;
use sqlx::any::AnyPoolOptions;
use sqlx::sqlite::SqlitePoolOptions;
use static_dir::static_dir;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use test_log::test;
use tokio::sync::Mutex;
use tokio_util::bytes::BufMut;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_http::LatencyUnit;
use tower_http::classify::ServerErrorsFailureClass;
use tower_http::trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse};
use tracing::{Level, Span, info_span};

#[derive(Debug, Clone)]
pub struct AppState {
    pub database: Database,
    pub servers: MinecraftServerCollection,
}

pub async fn run(state: AppState, config: config::Config) -> Result<(), anyhow::Error> {
    util::dirs::init_dirs().expect("Failed to initialize the data directory");

    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(tower_governor::key_extractor::SmartIpKeyExtractor)
            .per_millisecond((1000.0 / CONFIG.api_rate_limit) as u64)
            .burst_size((10.0 * CONFIG.api_rate_limit) as u32)
            .use_headers()
            .finish()
            .unwrap(),
    );
    let governor_limiter = governor_conf.limiter().clone();

    let governor_layer = tower_http::trace::TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::default().level(Level::INFO))
        .on_response(DefaultOnResponse::default().latency_unit(LatencyUnit::Micros));

    let interval = Duration::from_secs(60);
    // a separate background task to clean up governor
    thread::spawn(move || {
        loop {
            thread::sleep(interval);
            debug!("rate limiting storage size: {}", governor_limiter.len());
            governor_limiter.retain_recent();
        }
    });

    let check_free = Router::new()
        .route(
            "/username/{username}",
            get(api::handlers::get_username_valid),
        )
        .route(
            "/invite_link/{invite_link}",
            get(api::handlers::get_invite_valid),
        )
        .route(
            "/hostname/{hostname}",
            get(api::handlers::get_hostname_valid),
        );

    /// GET /session - session info (not implemented)
    /// POST /session - login
    /// DELETE /session - logout
    /// GET /session/user - get logged-in user info
    /// POST /session/user - create new user
    /// PATCH /session/user - change username/password (not implemented)
    /// DELETE /session/user - delete account (not implemented)
    let session = Router::new()
        .route(
            "/user",
            get(api::handlers::user_info)
                .post(api::handlers::user_register)
                .patch(|| async { StatusCode::NOT_IMPLEMENTED })
                .delete(|| async { StatusCode::NOT_IMPLEMENTED }),
        )
        .route(
            "/",
            get(|| async { StatusCode::NOT_IMPLEMENTED })
                .post(api::handlers::user_auth)
                .delete(api::handlers::logout),
        );

    let server = Router::new().route("/", get(api::handlers::server_info));

    let api = Router::new()
        .nest("/session", session)
        .nest("/server", server)
        .nest("/valid", check_free)
        .nest("/mods", Mod::routes())
        .nest("/versions", Version::routes())
        .nest("/mod_loaders", ModLoader::routes())
        .nest("/worlds", World::routes())
        .nest("/groups", Group::routes())
        .nest("/users", User::routes())
        .nest("/sessions", Session::routes())
        .nest("/invite_links", InviteLink::routes())
        .with_state(state);

    //TODO: include frontend

    let router = Router::new()
        .nest("/api", api)
        .layer(GovernorLayer {
            config: governor_conf,
        })
        .layer(governor_layer);

    let addr = format!("{}:{}", config.listen_address, config.listen_port);

    info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
