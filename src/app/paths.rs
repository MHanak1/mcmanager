use crate::app::args::ARGS;
use color_eyre::Result;
use std::{env, fs, path::PathBuf, sync::LazyLock};

pub fn create_dirs() -> Result<()> {
    fs::create_dir_all(&*WORKING_DIR)?;
    fs::create_dir_all(&*DATA_DIR)?;

    Ok(())
}

pub static WORKING_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    ARGS.working_dir.clone().unwrap_or(
    env::var("MCM_WORKING_DIR")
        .map(PathBuf::from)
        .unwrap_or(
            std::env::current_dir()
                .expect("Unable to find the working directory. You can overwrite it with the MCM_WORKING_DIR environment variable")
        )
    )
});

pub static DATA_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    ARGS.data_dir.clone().unwrap_or(
        env::var("MCM_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or(WORKING_DIR.join("data")),
    )
});

/// The name of the config file to be used, which may or may not include the file extension. This may also point to the directory in which the config file is stored.
pub static CONFIG: LazyLock<PathBuf> = LazyLock::new(|| {
    ARGS.config.clone().unwrap_or(
        env::var("MCM_CONFIG_FILE")
            .map(PathBuf::from)
            .unwrap_or(WORKING_DIR.join("config.toml")),
    )
});

pub fn config_files() -> Vec<PathBuf> {
    if CONFIG.is_dir() {
        let mut paths = vec![];
        for path in std::fs::read_dir(&*CONFIG)
            .expect("Failed to read config dir")
            .flatten()
        {
            paths.push(path.path());
        }
        paths
    } else {
        vec![CONFIG.clone()]
    }
}
