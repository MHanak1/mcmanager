use clap::Parser;
use color_eyre::eyre::{Ok, Result};
use mcmanager::app::config::Config;
use mcmanager::app::state::State;
use mcmanager::app::{args::ARGS, paths::CONFIG};
use sea_orm::EntityTrait;
use tracing::info;
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
        Config::generate_config_file(&*CONFIG)?;
        info!(
            "Successfully written the config file to: {}",
            CONFIG.display()
        );
        return Ok(());
    }

    let state = State::new().await?;
    //This doesn't work
    //state.watch_for_config_changes();

    //mcmanager::database::first_launch(&state).await?;

    println!(
        "{:#?}",
        mcmanager::database::Group::find()
            .one(&state.database())
            .await?
    );

    let mut config_changed = state.config().changed_from.clone();
    loop {
        //time::sleep(Duration::from_hours(1)).await;
        config_changed.changed().await.expect("dunno man");
    }
    //Ok(())
}
