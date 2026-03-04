use clap::Parser;
use color_eyre::eyre::Result;
use mcmanager::app::state::State;
use mcmanager::app::{args::ARGS, paths::CONFIG};
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
                .with_default_directive(tracing::level_filters::LevelFilter::ERROR.into())
                .with_env_var("MCM_LOG")
                .from_env_lossy(),
        )
        .init();
    //tracing_subscriber::fmt().init();

    mcmanager::app::paths::create_dirs()?;
    if ARGS.gen_config {
        let _ = mcmanager::app::config::generate_config_file(&*CONFIG)
            .inspect_err(|err| error!("Could not create config: {err}"));
    }

    let state = State::new().await?;

    let mut config_changed = state.config.state.config_changed.subscribe();
    loop {
        config_changed.changed().await.expect("dunno man");
        println!("haii");
    }
    //Ok(())
}
