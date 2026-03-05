use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::Result;
use mcmanager::app::config::Config;
use mcmanager::app::state::State;
use mcmanager::app::{args::ARGS, paths::CONFIG};
use tokio::time;
use tracing::error;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    _ = mcmanager::app::args::Args::parse();
    color_eyre::install()?;
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
                .with_env_var("MCM_LOG")
                .from_env_lossy(),
        )
        .init();
    //tracing_subscriber::fmt().init();

    mcmanager::app::paths::create_dirs()?;
    if ARGS.gen_config {
        let _ = Config::generate_config_file(&*CONFIG)
            .inspect_err(|err| error!("Could not create config: {err}"));
    }

    let state = State::new().await?;

    let mut config_changed = state.config.changed_from.clone();
    loop {
        //time::sleep(Duration::from_hours(1)).await;
        println!("Main czeka na zmiany configu");
        config_changed.changed().await.expect("dunno man");
        println!("Main widzi zmiany configu");
    }
    //Ok(())
}
