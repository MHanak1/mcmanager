use crate::api::handlers::ApiObject;
use crate::config;
use crate::config::CONFIG;
use crate::database::Database;
use crate::database::objects::{Group, InviteLink, Mod, ModLoader, Session, User, Version, World};
use crate::minecraft::server::MinecraftServerCollection;
use crate::{api, util};
use axum::routing::{get, post};
use axum::{Router};
use log::{debug, info};
use reqwest::StatusCode;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use socketioxide::SocketIoBuilder;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse};
use tracing::Level;
use uuid::Uuid;
use crate::api::socketio::console_socketio;
use crate::database::types::Id;

#[derive(Clone)]
pub struct AppState {
    pub database: Database,
    pub servers: MinecraftServerCollection,
    // i am not using a moka cache because it's a good idea, i'm doing so because of my laziness.
    pub console_tickets: moka::future::Cache<Uuid, Id>
}

pub async fn run(state: AppState, config: config::Config) -> Result<(), color_eyre::eyre::Error> {
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

    let trace_layer = tower_http::trace::TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::default().level(Level::INFO))
        .on_response(DefaultOnResponse::default().latency_unit(LatencyUnit::Millis));

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

    let console = Router::new()
        .route(
            "/ticket",
            post(api::handlers::generate_console_ticket)
        );


    // GET /session - session info (not implemented)
    // POST /session - login
    // DELETE /session - logout
    // GET /session/user - get logged-in user info
    // POST /session/user - create new user
    // PATCH /session/user - change username/password (not implemented)
    // DELETE /session/user - delete account (not implemented)
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

    let (socketio, io) = SocketIoBuilder::new().with_state(state.clone()).build_layer();

    io.ns("/ws/console", console_socketio);

    let server = Router::new().route("/", get(api::handlers::server_info));

    let api = Router::new()
        .nest("/session", session)
        .nest("/server", server)
        .nest("/valid", check_free)
        .nest("/console", console)
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
        .layer(socketio)
        .layer(GovernorLayer {
            config: governor_conf,
        })
        .layer(trace_layer);

    let addr = format!("{}:{}", config.listen_address, config.listen_port);

    info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
