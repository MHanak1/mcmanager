use std::fmt::Display;

use sea_orm::FromJsonQueryResult;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult)]
pub enum Loader {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
    Quilt,
}

impl Loader {
    pub const fn all() -> &'static [Loader] {
        &[
            Self::Vanilla,
            Self::Fabric,
            Self::Forge,
            Self::NeoForge,
            Self::Quilt,
        ]
    }

    pub const fn can_load_mods(&self) -> bool {
        match self {
            Self::Fabric | Self::Forge | Self::NeoForge | Self::Quilt => true,
            _ => false,
        }
    }

    pub const fn can_load_plugins(&self) -> bool {
        match self {
            _ => false,
        }
    }

    pub const fn modrinth_id(&self) -> &'static Option<&str> {
        match self {
            Self::Vanilla => &Some("minecraft"),
            Self::Fabric => &Some("fabric"),
            Self::Forge => &Some("forge"),
            Self::NeoForge => &Some("neoforge"),
            Self::Quilt => &Some("quilt"),
        }
    }
}

impl Display for Loader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vanilla => write!(f, "Vanilla"),
            Self::Fabric => write!(f, "Fabric"),
            Self::Forge => write!(f, "Forge"),
            Self::NeoForge => write!(f, "NeoForge"),
            Self::Quilt => write!(f, "Quilt"),
        }
    }
}
