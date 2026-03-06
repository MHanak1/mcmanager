use std::{path::PathBuf, sync::LazyLock};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Generate the default config file
    #[arg(long)]
    pub gen_config: bool,

    /// The path to the config file
    ///
    /// The order of importance of configuration values is as the following:
    /// 1. Launch Arguments
    /// 2. Environment Variables (MCM_CATEGORY_VARIABLE)
    /// 3. Config file
    /// 4. Defauld values
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub working_dir: Option<PathBuf>,

    #[arg(long)]
    pub data_dir: Option<PathBuf>,
}

/// The name of the config file to be used, which may or may not include the file extension
pub static ARGS: LazyLock<Args> = LazyLock::new(Args::parse);
