use anyhow::Result;
use argon2::password_hash::SaltString;
use mcmanager::database::objects::{DbObject, Session, User, World};
use mcmanager::database::types::{Id, Token};
use mcmanager::database::{Database, objects};
use mcmanager::util;
use rand::rngs::OsRng;
use std::path::Path;

fn main() -> Result<()> {
    util::dirs::init_dirs().expect("Failed to initialize the data directory");
    let conn = rusqlite::Connection::open(Path::new(&util::dirs::data_dir().join("database.db")))?;
    let database = Database { conn };
    database.init().expect("Failed to init database");

    let miguel = User {
        id: Default::default(),
        name: "MHanak".to_string(),
        avatar_id: None,
        memory_limit: None,
        player_limit: None,
        world_limit: None,
        active_world_limit: None,
        storage_limit: None,
        is_privileged: false,
        enabled: true,
    };

    let dingus = User {
        id: Default::default(),
        name: "Dingus".to_string(),
        avatar_id: None,
        memory_limit: None,
        player_limit: None,
        world_limit: None,
        active_world_limit: None,
        storage_limit: None,
        is_privileged: false,
        enabled: true,
    };

    database.insert(&miguel)?;
    database.insert(&dingus)?;

    let miguel_token = Token::new(4);
    let dingus_token = Token::new(4);

    println!("miguel token: {}", miguel_token);
    println!("dingus token: {}", dingus_token);

    database.insert(&Session {
        user_id: miguel.id,
        token: miguel_token,
        created: Default::default(),
        expires: false,
    })?;

    database.insert(&Session {
        user_id: dingus.id,
        token: dingus_token,
        created: Default::default(),
        expires: false,
    })?;

    let forge = objects::ModLoader {
        id: Id::new_random(),
        name: "Forge".to_string(),
        can_load_mods: false,
    };

    let fabric = objects::ModLoader {
        id: Id::new_random(),
        name: "Fabric".to_string(),
        can_load_mods: false,
    };

    let fabric1214 = objects::Version {
        id: Id::new_random(),
        minecraft_version: "1.21.4".to_string(),
        mod_loader_id: fabric.id,
    };

    let forge1122 = objects::Version {
        id: Id::new_random(),
        minecraft_version: "1.12.2".to_string(),
        mod_loader_id: forge.id,
    };

    database.insert(&forge)?;
    database.insert(&fabric)?;
    database.insert(&forge1122)?;
    database.insert(&fabric1214)?;

    database.insert(&World {
        id: Default::default(),
        owner_id: miguel.id,
        name: "Miguel's world".to_string(),
        icon_id: None,
        allocated_memory: 1024,
        version_id: fabric1214.id,
        enabled: true,
    })?;

    database.insert(&World {
        id: Default::default(),
        owner_id: miguel.id,
        name: "Fucky Wucky world".to_string(),
        icon_id: None,
        allocated_memory: 102400,
        version_id: forge1122.id,
        enabled: false,
    })?;

    database.insert(&World {
        id: Default::default(),
        owner_id: dingus.id,
        name: "Dingusland".to_string(),
        icon_id: None,
        allocated_memory: 1,
        version_id: fabric1214.id,
        enabled: false,
    })?;

    database.insert(&objects::Mod {
        id: Id::new_random(),
        version_id: fabric1214.id,
        name: "Fabric API".to_string(),
        description: "Lightweight and modular API providing common hooks and intercompatibility measures utilized by mods using the Fabric toolchain.".to_string(),
        icon_id: None,
        hidden: false,
    })?;

    database.insert(&objects::Mod {
        id: Id::new_random(),
        version_id: fabric1214.id,
        name: "Sodium".to_string(),
        description: "The fastest and most compatible rendering optimization mod for Minecraft. Now available for both NeoForge and Fabric!".to_string(),
        icon_id: None,
        hidden: false,
    })?;

    database.insert(&objects::Mod {
        id: Id::new_random(),
        version_id: fabric1214.id,
        name: "Cloth Config API".to_string(),
        description: "Configuration Library for Minecraft Mods".to_string(),
        icon_id: None,
        hidden: false,
    })?;

    database.insert(&objects::Mod {
        id: Id::new_random(),
        version_id: fabric1214.id,
        name: "Iris Shaders".to_string(),
        description: "A modern shader pack loader for Minecraft intended to be compatible with existing OptiFine shader packs".to_string(),
        icon_id: None,
        hidden: false,
    })?;

    database.insert(&objects::Mod {
        id: Id::new_random(),
        version_id: forge1122.id,
        name: "Just Enough Items (JEI)".to_string(),
        description: "View Items and Recipes".to_string(),
        icon_id: None,
        hidden: false,
    })?;

    let geckolib = objects::Mod {
        id: Id::new_random(),
        version_id: forge1122.id,
        name: "GeckoLib".to_string(),
        description: "A 3D animation library for entities, blocks, items, armor, and more!"
            .to_string(),
        icon_id: None,
        hidden: false,
    };

    database.insert(&geckolib)?;
    let mut stmt = database
        .conn
        .prepare("SELECT id, version_id, name, description, icon_id, hidden FROM mods")?;
    let mods_iter = stmt.query_map([], objects::Mod::from_row)?;

    for mcmod in mods_iter {
        let mcmod = mcmod?;

        let version = objects::Version::get_from_db(&database.conn, mcmod.version_id)?;
        let mod_loader = objects::ModLoader::get_from_db(&database.conn, version.mod_loader_id);
        println!("{} ({:b})", mcmod.name, mcmod.id.as_i64());
        println!("    {}", mcmod.description);
        println!("    {} {}", mod_loader?.name, version.minecraft_version);
        //println!("{}", serde_json::to_string_pretty(&mcmod)?);
        //println!("{mcmod:#?}");
    }

    Ok(())
}
