use axum::{
    Json,
    extract::{Extension, Query},
    response::IntoResponse,
};
use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::IntoParams;

use crate::{AppState, auth::AuthUser, errors::ApiError, models::HomeSummaryResponse};

#[derive(Debug, Deserialize, IntoParams)]
pub struct HomeQuery {
    pub date: Option<NaiveDate>,
}

#[utoipa::path(
    get,
    path = "/v1/home",
    params(
        HomeQuery
    ),
    responses(
        (status = 200, description = "Today's planned and completed minutes summary", body = HomeSummaryResponse),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "Habits"
)]
pub async fn get_home(
    Extension(state): Extension<Arc<AppState>>,
    auth_user: AuthUser,
    Query(query): Query<HomeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // 1. Determine target date (explicit query date or today's date)
    let target_date = query.date.unwrap_or_else(|| Utc::now().date_naive());

    // 2. Planned minutes:
    //    - Active daily actions duration
    //    - Scheduled todos duration for target_date
    let planned_actions_minutes: i64 = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(duration_minutes), 0)::BIGINT
        FROM daily_actions
        WHERE user_id = $1 AND is_archived = FALSE
        "#,
        auth_user.id
    )
    .fetch_one(&state.pool)
    .await?
    .unwrap_or(0);

    let planned_todos_minutes: i64 = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(duration_minutes), 0)::BIGINT
        FROM todos
        WHERE user_id = $1 AND target_date = $2
        "#,
        auth_user.id,
        target_date
    )
    .fetch_one(&state.pool)
    .await?
    .unwrap_or(0);

    let total_planned = planned_actions_minutes + planned_todos_minutes;

    // 3. Completed minutes:
    //    - Done action logs for target_date
    //    - Completed todos for target_date
    let completed_actions_minutes: i64 = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(da.duration_minutes), 0)::BIGINT
        FROM action_logs al
        JOIN daily_actions da ON da.id = al.action_id
        WHERE al.user_id = $1 AND al.log_date = $2 AND al.status = 'done'
        "#,
        auth_user.id,
        target_date
    )
    .fetch_one(&state.pool)
    .await?
    .unwrap_or(0);

    let completed_todos_minutes: i64 = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(duration_minutes), 0)::BIGINT
        FROM todos
        WHERE user_id = $1 AND target_date = $2 AND is_completed = TRUE
        "#,
        auth_user.id,
        target_date
    )
    .fetch_one(&state.pool)
    .await?
    .unwrap_or(0);

    let total_completed = completed_actions_minutes + completed_todos_minutes;

    Ok(Json(HomeSummaryResponse {
        date: target_date,
        planned_minutes: total_planned as i32,
        completed_minutes: total_completed as i32,
    }))
}
