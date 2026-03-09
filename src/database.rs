use color_eyre::Result;
use color_eyre::eyre::ContextCompat;
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ConnectOptions, Database, DatabaseConnection, EntityTrait,
    FromJsonQueryResult, IntoActiveModel, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info};
use validator::Validate;

use crate::app::paths::DATA_DIR;
use crate::app::state::AppState;
use crate::{app, util};

pub mod entity;
pub mod enums;
pub use entity::*;
pub use enums::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult, Default)]
pub struct StringVec(pub Vec<String>);

impl From<Vec<String>> for StringVec {
    fn from(value: Vec<String>) -> Self {
        Self(value)
    }
}

pub async fn create_database(config: &app::config::Database) -> Result<DatabaseConnection> {
    let url = if config.connection == "sqlite" {
        format!("sqlite://{}/database.sqlite?mode=rwc", DATA_DIR.display())
    } else {
        config.connection.clone()
    };
    debug!("Connecting to database at: {url}");

    let mut opt = ConnectOptions::new(&url);
    opt.max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .connect_timeout(config.connect_timeout)
        .acquire_timeout(config.acquire_timeout)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime)
        .sqlx_logging_level(log::LevelFilter::Debug);

    let db = Database::connect(opt).await?;
    db.get_schema_registry("mcmanager::database::entity::*")
        .sync(&db)
        .await?;

    info!("Successfully connected to the database at: {url}");

    Ok(db)
}

pub async fn first_launch(state: &AppState) -> Result<()> {
    let transaction = state.database().begin().await?;

    let selected_loaders = requestty::prompt_one(
        requestty::Question::multi_select("loaders")
            .message("Select loaders to add to the database")
            .choices_with_default(vec![
                ("Vanilla", true),
                ("Fabric", true),
                ("Forge", true),
                ("NeoForge", true),
                ("Quilt", true),
                // ("Babric", false),
                // ("BTA (Babric)", false),
                // ("Java-agent", false),
                // ("Legaclegacy-fabric", false),
                // ("LiteLoloader", false),
                // ("Risugamr", false),
                // ("NilLoadeer", false),
                // ("Ornithe", false),
                // ("Rift", false),
            ])
            .build(),
    )?;

    let selected_loaders: Vec<String> = selected_loaders
        .as_list_items()
        .context("Somehow a list is not a list")?
        .iter()
        .map(|item| item.text.clone())
        .collect();

    let selected_loaders = data::ActiveModel {
        key: Set("enabled_loaders".into()),
        value: Set(json!(selected_loaders)),
    };

    selected_loaders.insert(&transaction).await?;

    let admin_username_and_password = requestty::prompt(vec![
        requestty::Question::input("username")
            .message("Enter the username for the admin user")
            .default("Admin")
            .validate(|username, _| {
                util::validate_username(username).map_err(|err| err.to_string())
            })
            .build(),
        requestty::Question::password("password")
            .message("Enter a password")
            .mask('*')
            .validate(|password, _| {
                util::validate_password(password).map_err(|err| err.to_string())
            })
            .build(),
        requestty::Question::password("password_repeat")
            .message("Repeat password")
            .mask('*')
            .validate_on_key(|ans, _| !ans.is_empty())
            .validate(|password_repeat, previous_answers| {
                util::validate_password(password_repeat).map_err(|err| err.to_string())?;
                let a = &previous_answers["password"];
                if a.as_string().expect("Huh") == password_repeat {
                    Ok(())
                } else {
                    Err("Passwords don't match".to_owned())
                }
            })
            .build(),
    ])?;

    let admin_group = PartialGroup {
        slug: util::sanitise_slug(admin_username_and_password["username"].as_string().unwrap()),
        name: admin_username_and_password["username"]
            .as_string()
            .unwrap()
            .to_owned(),
        permissions: group::Permissions {
            is_privileged: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let admin_group = admin_group.into_active_model().insert(&transaction).await?;

    let admin = PartialUser {
        slug: util::sanitise_slug(admin_username_and_password["username"].as_string().unwrap()),
        username: admin_username_and_password["username"]
            .as_string()
            .unwrap()
            .to_owned(),
        group_id: admin_group.id,
        ..Default::default()
    };

    admin.validate()?;

    let admin = admin.into_active_model().insert(&transaction).await?;

    let password = PartialPassword::new(
        admin.id,
        admin_username_and_password["password"].as_string().unwrap(),
    )?;

    password.into_active_model().insert(&transaction).await?;

    let default_group = PartialGroup {
        slug: util::sanitise_slug("Default"),
        name: "Default".to_owned(),
        limits: group::Limits {
            per_world_memory_limit: Some(2048),
            active_world_limit: Some(3),
            world_limit: Some(20),
            ..Default::default()
        },
        ..Default::default()
    };

    let default_group = default_group
        .into_active_model()
        .insert(&transaction)
        .await?;

    Data::insert(data::ActiveModel {
        key: Set("default_group".to_owned()),
        value: Set(json!(default_group.slug)),
    })
    .exec(&transaction)
    .await?;

    Data::insert(data::ActiveModel {
        key: Set("completed_setup".to_owned()),
        value: Set(json!(true)),
    })
    .exec(&transaction)
    .await?;

    transaction.commit().await?;

    info!("Successfully initialised the database");

    Ok(())
}
