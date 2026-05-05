use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::UserId;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Normal,
    Privileged,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
}
