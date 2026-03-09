use chrono::{DateTime, Utc};
use seaography::CustomOutputType;
use uuid::Uuid;

#[derive(Clone, CustomOutputType)]
pub struct CreatedSession {
    pub id: Uuid,

    pub user_id: Uuid,

    pub token: Uuid,

    pub created: DateTime<Utc>,

    pub expires: bool,
}
