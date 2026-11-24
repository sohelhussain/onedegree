use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    User,
    Subadmin,
    Admin,
}

impl Default for UserRole {
    fn default() -> Self {
        Self::User
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub email: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub full_name: Option<String>,
    pub avatar_url: Option<String>,
    pub timezone: String,
    pub coin_balance: i32,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub role: UserRole,
    pub is_premium: bool,
    pub premium_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub fn has_premium(user: &User) -> bool {
    if user.role == UserRole::Admin {
        return true;
    }
    if user.is_premium {
        match user.premium_until {
            None => true,
            Some(until) => until > Utc::now(),
        }
    } else {
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: Option<String>,
    pub full_name: Option<String>,
    pub avatar_url: Option<String>,
    pub timezone: String,
    pub coin_balance: i32,
    pub role: UserRole,
    pub email_verified: bool,
    pub has_premium: bool,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        let is_verified = u.email_verified_at.is_some();
        let premium = has_premium(&u);
        Self {
            id: u.id,
            email: u.email,
            full_name: u.full_name,
            avatar_url: u.avatar_url,
            timezone: u.timezone,
            coin_balance: u.coin_balance,
            role: u.role,
            email_verified: is_verified,
            has_premium: premium,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateUserRequest {
    pub timezone: Option<String>,
    pub full_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub user: UserResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RefreshResponse {
    pub access_token: String,
    pub expires_in: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HomeSummaryResponse {
    pub date: NaiveDate,
    pub planned_minutes: i32,
    pub completed_minutes: i32,
}
