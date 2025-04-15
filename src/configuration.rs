use config::Config;

use once_cell::sync::Lazy;

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    Config::builder()
        .add_source(config::File::from_str(
            include_str!("resources/default_config.toml"),
            config::FileFormat::Toml,
        ))
        .build()
        .expect("failed to parse config")
});
