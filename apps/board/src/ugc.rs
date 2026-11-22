//! Banned words (금칙어), ported from AI-DO domains/ugc_policy/content_filter.py
//! and the filter-term routes of domains/admin/dm_safety_router.py.
//!
//! A board manager's term rejects a post or comment whose normalized text
//! contains it: NFKC, case folding, then whitespace, invisible characters and
//! separator punctuation removed. No AI judgement, no spelling variants; the
//! rejection never says which term matched. AI-DO's terms-of-use acceptance
//! (`ugc_policy/terms.py`) is not ported (docs/PORTING.md §1).
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use unicode_normalization::UnicodeNormalization;

use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult, check_length},
    managers::require_manager,
    time::now_utc,
};

pub const FILTER_TERMS_MAX: i64 = 1000;
pub const FILTER_TERM_MAX_LENGTH: usize = 50;

/// Zero-width spaces, direction marks, word joiners and the BOM.
fn invisible(c: char) -> bool {
    matches!(c as u32, 0x200B..=0x200F | 0x2060..=0x2064 | 0xFEFF)
}

fn separator(c: char) -> bool {
    ".-_*/|~^+=,:;!?()[]{}<>'\"\u{00B7}\u{2022}`\\".contains(c)
}

/// Python's `\s` for str patterns (`str.isspace`): Unicode White_Space plus
/// the information separators U+001C..U+001F.
fn python_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1C}'..='\u{1F}').contains(&c)
}

pub fn normalize_text(value: &str) -> String {
    let nfkc: String = value.nfkc().collect();
    caseless::default_case_fold_str(&nfkc)
        .chars()
        .filter(|&c| !(python_space(c) || invisible(c) || separator(c)))
        .collect()
}

pub fn contains_filtered_term<S: AsRef<str>>(text: &str, normalized_terms: &[S]) -> bool {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        return false;
    }
    normalized_terms.iter().any(|term| {
        let term = term.as_ref();
        !term.is_empty() && normalized.contains(term)
    })
}

/// 422 `ugc.content_rejected` when any text holds a banned word.
pub async fn ensure_content_allowed(pool: &PgPool, texts: &[&str]) -> ApiResult<()> {
    let candidates: Vec<&str> = texts.iter().copied().filter(|t| !t.is_empty()).collect();
    if candidates.is_empty() {
        return Ok(());
    }
    let terms: Vec<String> = sqlx::query_scalar("SELECT normalized FROM ugc_filter_terms")
        .fetch_all(pool)
        .await?;
    if terms.is_empty() {
        return Ok(());
    }
    if candidates
        .iter()
        .any(|text| contains_filtered_term(text, &terms))
    {
        return Err(content_rejected());
    }
    Ok(())
}

pub fn content_rejected() -> ApiError {
    ApiError::unprocessable(
        "ugc.content_rejected",
        "This contains wording that is not allowed by the content rules.",
    )
}

#[derive(Serialize, sqlx::FromRow)]
pub struct FilterTermItem {
    id: String,
    term: String,
    #[serde(serialize_with = "crate::time::serialize")]
    created_at: chrono::NaiveDateTime,
}

#[derive(Serialize)]
pub struct FilterTermList {
    items: Vec<FilterTermItem>,
    limit: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterTermCreateRequest {
    term: String,
}

pub async fn list_filter_terms(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<FilterTermList>> {
    require_manager(&state.pool, &identity).await?;
    let items = sqlx::query_as::<_, FilterTermItem>(
        "SELECT id, term, created_at FROM ugc_filter_terms ORDER BY created_at DESC, id",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(FilterTermList {
        items,
        limit: FILTER_TERMS_MAX,
    }))
}

pub async fn add_filter_term(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<FilterTermCreateRequest>,
) -> ApiResult<impl IntoResponse> {
    require_manager(&state.pool, &identity).await?;
    check_length("term", &payload.term, 1, FILTER_TERM_MAX_LENGTH)?;
    let cleaned = payload.term.trim().to_string();
    let normalized = normalize_text(&cleaned);
    // NFKC and case folding can lengthen text (㍿ → 株式会社). Only a
    // normalized value inside the limit is accepted, never a truncated one.
    if cleaned.is_empty()
        || normalized.is_empty()
        || cleaned.chars().count() > FILTER_TERM_MAX_LENGTH
        || normalized.chars().count() > FILTER_TERM_MAX_LENGTH
    {
        return Err(ApiError::unprocessable(
            "ugc.filter_term_invalid",
            format!(
                "Filter terms must be 1 to {FILTER_TERM_MAX_LENGTH} characters. Ligatures and squared characters (such as ㍿) count as their expanded form."
            ),
        ));
    }
    let mut tx = state.pool.begin().await?;
    // Serialize additions so the limit holds under concurrent requests.
    sqlx::query("LOCK TABLE ugc_filter_terms IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM ugc_filter_terms")
        .fetch_one(&mut *tx)
        .await?;
    if count >= FILTER_TERMS_MAX {
        return Err(ApiError::unprocessable(
            "ugc.filter_term_limit",
            format!("You can register up to {FILTER_TERMS_MAX} filter terms."),
        ));
    }
    let item = sqlx::query_as::<_, FilterTermItem>(
        "INSERT INTO ugc_filter_terms (id, term, normalized, created_by_id, created_at) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (normalized) DO NOTHING \
         RETURNING id, term, created_at",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&cleaned)
    .bind(&normalized)
    .bind(&identity.user_id)
    .bind(now_utc())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        ApiError::conflict(
            "ugc.filter_term_duplicate",
            "This filter term already exists.",
        )
    })?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(item)))
}

pub async fn remove_filter_term(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(term_id): Path<String>,
) -> ApiResult<StatusCode> {
    require_manager(&state.pool, &identity).await?;
    let removed = sqlx::query("DELETE FROM ugc_filter_terms WHERE id = $1")
        .bind(&term_id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(ApiError::not_found(
            "ugc.filter_term_not_found",
            "The filter term was not found.",
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    // AI-DO tests/test_ugc_policy.py
    #[test]
    fn normalization_catches_spacing_case_width_and_invisible_characters() {
        let terms = [normalize_text("badword")];
        for variant in [
            "BadWord",
            "b a d w o r d",
            "b.a.d-w_o*r/d",
            "ｂａｄｗｏｒｄ",
            "bad\u{200B}word",
        ] {
            assert!(
                contains_filtered_term(&format!("그 말은 {variant} 이다"), &terms),
                "{variant:?}"
            );
        }
        assert!(!contains_filtered_term("good words only", &terms));
        assert!(!contains_filtered_term("", &terms));
        let korean = [normalize_text("나쁜말")];
        assert!(contains_filtered_term("나 쁜\u{feff}말", &korean));
    }

    #[test]
    fn known_gaps_are_not_claimed() {
        // Spelling variants (digits, look-alike letters) are not caught.
        let terms = [normalize_text("badword")];
        assert!(!contains_filtered_term("b4dw0rd", &terms));
    }

    #[test]
    fn filter_term_longer_than_the_limit_after_normalization_is_rejected_not_truncated() {
        let term = "㍿".repeat(13);
        assert!(term.chars().count() <= FILTER_TERM_MAX_LENGTH);
        assert!(normalize_text(&term).chars().count() > FILTER_TERM_MAX_LENGTH);
    }

    #[test]
    fn case_folding_is_full_folding_like_python_casefold() {
        assert_eq!(normalize_text("Straße"), "strasse");
        assert_eq!(normalize_text("ΣΊΣΥΦΟΣ"), normalize_text("σίσυφος"));
        assert_eq!(normalize_text("a\u{1c}b"), "ab");
    }
}
