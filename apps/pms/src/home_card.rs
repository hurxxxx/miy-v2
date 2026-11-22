//! The portal home's "내 업무" card (miy platform → app call, manifest
//! `platformEndpoints.homeCard`): the person's open tasks due today or
//! overdue, in their time zone, like AI-DO's Today & Overdue view. Read only;
//! platform calls only.
use axum::{Extension, Json, extract::State};
use chrono::{NaiveDate, Utc};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::ApiResult;
use crate::notify::{platform_only, task_path};
use crate::tasks::assigned_open_tasks;

const MAX_ITEMS: usize = 5;

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeCardRequest {
    /// IANA time zone. Missing and unknown values use Asia/Seoul.
    #[serde(default)]
    time_zone: Option<String>,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct HomeCardResponse {
    title: String,
    count: usize,
    #[schema(max_items = 5)]
    items: Vec<HomeCardItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct HomeCardItem {
    title: String,
    subtitle: String,
    path: String,
}

/// Today in the person's zone (`Asia/Seoul` for a missing or unknown name).
pub fn local_today(time_zone: Option<&str>) -> NaiveDate {
    let zone: chrono_tz::Tz = time_zone
        .and_then(|name| name.parse().ok())
        .unwrap_or(chrono_tz::Asia::Seoul);
    Utc::now().with_timezone(&zone).date_naive()
}

/// One task line: `(title, list key + number, due date)`.
pub struct CardTask {
    pub id: String,
    pub title: String,
    pub reference: String,
    pub due: NaiveDate,
}

pub fn card(tasks: &[CardTask], today: NaiveDate, english: bool) -> HomeCardResponse {
    let due_today = tasks.iter().filter(|task| task.due == today).count();
    let overdue = tasks.len() - due_today;
    let title = if english {
        format!("My tasks · today {due_today} · overdue {overdue}")
    } else {
        format!("내 업무 · 오늘 {due_today} · 지연 {overdue}")
    };
    let items = tasks
        .iter()
        .take(MAX_ITEMS)
        .map(|task| {
            let days = (today - task.due).num_days();
            let when = match (days, english) {
                (0, false) => "오늘 마감".to_string(),
                (0, true) => "Due today".to_string(),
                (n, false) => format!("{n}일 지연"),
                (1, true) => "1 day overdue".to_string(),
                (n, true) => format!("{n} days overdue"),
            };
            HomeCardItem {
                title: task.title.clone(),
                subtitle: format!("{} · {when}", task.reference),
                path: task_path(&task.id),
            }
        })
        .collect();
    HomeCardResponse {
        title,
        count: tasks.len(),
        items,
    }
}

/// `POST /v1/home-card`.
#[utoipa::path(
    post, path = "/v1/home-card", operation_id = "getPmsHomeCard", tag = "Platform callbacks",
    description = "Platform-only, read-only home card for open tasks due today or overdue in the caller’s time zone.",
    request_body = ref("#/components/schemas/HomeCardRequest"),
    responses(
        (status = 200, description = "Success", body = HomeCardResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 400, description = "Malformed JSON", body = String, content_type = "text/plain"),
        (status = 403, description = "Current permissions do not allow the operation", body = crate::error::ErrorResponse),
        (status = 413, description = "JSON body exceeds the extractor limit", body = String, content_type = "text/plain"),
        (status = 415, description = "Expected application/json", body = String, content_type = "text/plain"),
        (status = 422, description = "JSON fields could not be decoded", body = String, content_type = "text/plain"),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn home_card(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Option<Json<HomeCardRequest>>,
) -> ApiResult<Json<HomeCardResponse>> {
    if !identity.context.platform {
        return Err(platform_only());
    }
    let request = body.map(|Json(body)| body).unwrap_or_default();
    let today = local_today(request.time_zone.as_deref());
    let mut conn = pool.acquire().await?;
    let (tasks, context) = assigned_open_tasks(&mut conn, &identity.user_id, Some(today)).await?;
    let lines: Vec<CardTask> = tasks
        .iter()
        .map(|task| CardTask {
            id: task.id.clone(),
            title: task.title.clone(),
            reference: context.reference(task),
            due: task.due_date.unwrap_or(today),
        })
        .collect();
    let english = request.locale.as_deref() == Some("en-US");
    Ok(Json(card(&lines, today, english)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_counts_today_and_overdue_and_keeps_five_items() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        let tasks: Vec<CardTask> = (0..7)
            .map(|i| CardTask {
                id: format!("t{i}"),
                title: format!("task {i}"),
                reference: format!("WEB-{i}"),
                due: today - chrono::Duration::days(if i < 2 { 0 } else { i }),
            })
            .collect();
        let value = serde_json::to_value(card(&tasks, today, false)).unwrap();
        assert_eq!(value["title"], "내 업무 · 오늘 2 · 지연 5");
        assert_eq!(value["count"], 7);
        assert_eq!(value["items"].as_array().unwrap().len(), MAX_ITEMS);
        assert_eq!(value["items"][0]["subtitle"], "WEB-0 · 오늘 마감");
        assert_eq!(value["items"][2]["subtitle"], "WEB-2 · 2일 지연");
        assert_eq!(value["items"][2]["path"], "/tasks/t2");
        assert_eq!(
            serde_json::to_value(card(&tasks, today, true)).unwrap()["items"][2]["subtitle"],
            "WEB-2 · 2 days overdue"
        );
    }

    #[test]
    fn unknown_zones_fall_back_to_seoul() {
        assert_eq!(
            local_today(Some("Nowhere/City")),
            local_today(Some("Asia/Seoul"))
        );
    }
}
