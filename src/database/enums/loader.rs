use sea_orm::{DeriveActiveEnum, EnumIter, prelude::StringLen};
use strum::Display;

#[derive(
    EnumIter, DeriveActiveEnum, PartialEq, Display, Copy, Clone, Debug, Eq, PartialOrd, Ord,
)]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "lowercase"
)]
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
        matches!(
            self,
            Self::Fabric | Self::Forge | Self::NeoForge | Self::Quilt
        )
    }

    pub const fn can_load_plugins(&self) -> bool {
        false
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
