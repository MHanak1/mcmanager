use anyhow::Result;
use mcmanager::database::objects::{User, World};
use mcmanager::database::types::Id;
use mcmanager::database::{Database, objects};
use mcmanager::util;
use std::path::Path;

fn main() -> Result<()> {
    util::dirs::init_dirs().expect("Failed to initialize the data directory");
    let conn = rusqlite::Connection::open(Path::new(&util::dirs::data_dir().join("database.db")))?;
    let database = Database { conn };
    database.init().expect("Failed to init database");

    let miguel = database.create_user_from(
        User {
            id: Id::from_string("hhmVuPII")?,
            username: "MHanak".to_string(),
            is_privileged: true,
            ..Default::default()
        },
        "Password",
    )?;
    let _ = database.create_user_from(
        User {
            id: Id::from_string("h9htSMj6")?,
            username: "Dingus".to_string(),
            ..Default::default()
        },
        "AAAAAAAAAAAAAAA",
    )?;
    let _ = database.create_user_from(
        User {
            id: Id::from_string("IKzN1kgH")?,
            username: "Dorkus".to_string(),
            ..Default::default()
        },
        "A",
    )?;

    database
        .update(&miguel, None)
        .expect("Failed to update Miguel");

    let forge = objects::ModLoader {
        id: Id::from_string("Uz-rWPzk")?,
        name: "Forge".to_string(),
        can_load_mods: false,
    };

    let fabric = objects::ModLoader {
        id: Id::from_string("ZDkSeyGU")?,
        name: "Fabric".to_string(),
        can_load_mods: true,
    };

    let fabric1214 = objects::Version {
        id: Id::from_string("MWKefd0C")?,
        minecraft_version: "1.21.4".to_string(),
        mod_loader_id: fabric.id,
    };

    let forge1122 = objects::Version {
        id: Id::from_string("RPSz1TGj")?,
        minecraft_version: "1.12.2".to_string(),
        mod_loader_id: forge.id,
    };

    database.insert(&forge, None)?;
    database.insert(&fabric, None)?;
    database.insert(&forge1122, None)?;
    database.insert(&fabric1214, None)?;

    database.insert(
        &World {
            id: Id::from_string("BUcrnbMq").unwrap(),
            owner_id: miguel.id,
            name: "Miguel's world".to_string(),
            hostname: "miguel".to_string(),
            icon_id: None,
            allocated_memory: 1024,
            version_id: fabric1214.id,
            enabled: true,
        },
        None,
    )?;

    database.insert(
        &World {
            id: Id::from_string("GkAgtAmd").unwrap(),
            owner_id: miguel.id,
            name: "Fucky Wucky world".to_string(),
            hostname: "fky-wky".to_string(),
            icon_id: None,
            allocated_memory: 102400,
            version_id: forge1122.id,
            enabled: false,
        },
        None,
    )?;

    database.insert(
        &World {
            id: Id::from_string("Iar-FBHJ").unwrap(),
            owner_id: miguel.id,
            name: "Dingusland".to_string(),
            hostname: "dingusland".to_string(),
            icon_id: None,
            allocated_memory: 2048,
            version_id: fabric1214.id,
            enabled: false,
        },
        None,
    )?;

    database.insert(&objects::Mod {
        id: Id::new_random(),
        version_id: fabric1214.id,
        name: "Fabric API".to_string(),
        description: "Lightweight and modular API providing common hooks and intercompatibility measures utilized by mods using the Fabric toolchain.".to_string(),
        icon_id: None,
        owner_id: miguel.id,
    }, None)?;

    database.insert(&objects::Mod {
        id: Id::new_random(),
        version_id: fabric1214.id,
        name: "Sodium".to_string(),
        description: "The fastest and most compatible rendering optimization mod for Minecraft. Now available for both NeoForge and Fabric!".to_string(),
        icon_id: None,
        owner_id: miguel.id,
    }, None)?;

    database.insert(
        &objects::Mod {
            id: Id::new_random(),
            version_id: fabric1214.id,
            name: "Cloth Config API".to_string(),
            description: "Configuration Library for Minecraft Mods".to_string(),
            icon_id: None,
            owner_id: miguel.id,
        },
        None,
    )?;

    database.insert(&objects::Mod {
        id: Id::new_random(),
        version_id: fabric1214.id,
        name: "Iris Shaders".to_string(),
        description: "A modern shader pack loader for Minecraft intended to be compatible with existing OptiFine shader packs".to_string(),
        icon_id: None,
        owner_id: miguel.id,
    }, None)?;

    database.insert(
        &objects::Mod {
            id: Id::new_random(),
            version_id: forge1122.id,
            name: "Just Enough Items (JEI)".to_string(),
            description: "View Items and Recipes".to_string(),
            icon_id: None,
            owner_id: miguel.id,
        },
        None,
    )?;

    let geckolib = objects::Mod {
        id: Id::new_random(),
        version_id: forge1122.id,
        name: "GeckoLib".to_string(),
        description: "A 3D animation library for entities, blocks, items, armor, and more!"
            .to_string(),
        icon_id: None,
        owner_id: miguel.id,
    };

    database.insert(&geckolib, None)?;

    Ok(())
}
