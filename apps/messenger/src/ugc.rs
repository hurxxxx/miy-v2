//! AI-DO domains/ugc_policy: terms acceptance (terms.py), banned words
//! (content_filter.py) and the single gate before user-authored writes
//! (guard.py).
//!
//! miy: AI-DO read the terms version, switch, document URL and contact from
//! its settings (`AI_DO_UGC_*`); messenger keeps them in `dm_terms_settings`
//! (AI-DO defaults), changed by messenger managers.
use crate::{
    audit,
    error::{ApiError, ApiResult, is_unique_violation},
    schemas::{UgcContact, UgcTermsStatus},
    store::new_id,
    time::{iso, now_utc},
};
use chrono::NaiveDateTime;
use serde_json::json;
use sqlx::PgConnection;
use unicode_normalization::UnicodeNormalization;

pub const FILTER_TERMS_MAX: i64 = 1000;
pub const FILTER_TERM_MAX_LENGTH: usize = 50;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TermsSettings {
    pub version: String,
    pub required: bool,
    pub document_url: Option<String>,
    pub contact_email: Option<String>,
    pub contact_url: Option<String>,
}

pub async fn settings(conn: &mut PgConnection) -> ApiResult<TermsSettings> {
    Ok(sqlx::query_as::<_, TermsSettings>(
        "SELECT version, required, document_url, contact_email, contact_url FROM dm_terms_settings WHERE id = 1",
    )
    .fetch_one(conn)
    .await?)
}

fn blank_to_none(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// `terms.terms_state(...).as_payload()`.
pub async fn terms_status(conn: &mut PgConnection, user_id: &str) -> ApiResult<UgcTermsStatus> {
    let settings = settings(conn).await?;
    let version = settings.version.trim().to_string();
    let accepted_at: Option<NaiveDateTime> = sqlx::query_scalar(
        "SELECT accepted_at FROM ugc_terms_acceptances WHERE user_id = $1 AND terms_version = $2",
    )
    .bind(user_id)
    .bind(&version)
    .fetch_optional(conn)
    .await?;
    let contact_email = blank_to_none(&settings.contact_email);
    let contact_url = blank_to_none(&settings.contact_url);
    Ok(UgcTermsStatus {
        version,
        required: settings.required,
        accepted: accepted_at.is_some(),
        accepted_at: accepted_at.map(iso),
        document_url: blank_to_none(&settings.document_url),
        contact: (contact_email.is_some() || contact_url.is_some()).then_some(UgcContact {
            support_email: contact_email,
            support_url: contact_url,
        }),
    })
}

/// `terms.accept_terms`: idempotent; 409 when the shown version is not current.
pub async fn accept_terms(
    conn: &mut PgConnection,
    user_id: &str,
    version: &str,
) -> ApiResult<(NaiveDateTime, bool)> {
    let expected = settings(conn).await?.version.trim().to_string();
    if version.trim() != expected {
        return Err(ApiError::conflict(
            "ugc_terms.version_mismatch",
            "The terms version changed. Read the current terms and accept again.",
        ));
    }
    let inserted: Option<String> = sqlx::query_scalar(
        "INSERT INTO ugc_terms_acceptances (user_id, terms_version, accepted_at, source) VALUES ($1, $2, $3, 'web') \
         ON CONFLICT (user_id, terms_version) DO NOTHING RETURNING user_id",
    )
    .bind(user_id)
    .bind(&expected)
    .bind(now_utc())
    .fetch_optional(&mut *conn)
    .await?;
    let changed = inserted.is_some();
    if changed {
        audit::record(
            conn,
            "ugc_terms.accepted",
            "user",
            Some(user_id),
            "사용자 작성 콘텐츠 이용규칙에 동의했다",
            user_id,
            json!({"version": expected, "source": "web"}),
        )
        .await?;
    }
    let accepted_at: NaiveDateTime = sqlx::query_scalar(
        "SELECT accepted_at FROM ugc_terms_acceptances WHERE user_id = $1 AND terms_version = $2",
    )
    .bind(user_id)
    .bind(&expected)
    .fetch_one(conn)
    .await?;
    Ok((accepted_at, changed))
}

/// `terms.require_terms_accepted`.
pub async fn require_terms_accepted(conn: &mut PgConnection, user_id: &str) -> ApiResult<()> {
    let settings = settings(conn).await?;
    if !settings.required {
        return Ok(());
    }
    let found: Option<String> = sqlx::query_scalar(
        "SELECT user_id FROM ugc_terms_acceptances WHERE user_id = $1 AND terms_version = $2",
    )
    .bind(user_id)
    .bind(settings.version.trim())
    .fetch_optional(conn)
    .await?;
    if found.is_none() {
        return Err(ApiError::forbidden(
            "ugc_terms.acceptance_required",
            "Accept the user content terms before writing.",
        ));
    }
    Ok(())
}

fn removable(c: char) -> bool {
    // Python `\s` (str.isspace) plus AI-DO's invisible and separator lists.
    c.is_whitespace()
        || ('\u{1c}'..='\u{1f}').contains(&c)
        || ('\u{200B}'..='\u{200F}').contains(&c)
        || ('\u{2060}'..='\u{2064}').contains(&c)
        || c == '\u{FEFF}'
        || ".-_*/|~^+=,:;!?()[]{}<>'\"\u{00B7}\u{2022}`\\".contains(c)
}

/// `content_filter.normalize_text`: NFKC, casefold, then drop whitespace,
/// invisible characters and separators. Python's casefold is approximated by
/// Unicode lowercase plus the `ß` expansion.
pub fn normalize_text(value: &str) -> String {
    value
        .nfkc()
        .collect::<String>()
        .to_lowercase()
        .replace('ß', "ss")
        .chars()
        .filter(|c| !removable(*c))
        .collect()
}

pub fn contains_filtered_term(text: &str, normalized_terms: &[String]) -> bool {
    let normalized = normalize_text(text);
    !normalized.is_empty()
        && normalized_terms
            .iter()
            .any(|term| !term.is_empty() && normalized.contains(term.as_str()))
}

/// `content_filter.ensure_content_allowed`: 422 `ugc.content_rejected`
/// without naming the matched term.
pub async fn ensure_content_allowed(conn: &mut PgConnection, texts: &[&str]) -> ApiResult<()> {
    let candidates: Vec<&&str> = texts.iter().filter(|t| !t.is_empty()).collect();
    if candidates.is_empty() {
        return Ok(());
    }
    let terms: Vec<String> = sqlx::query_scalar("SELECT normalized FROM ugc_filter_terms")
        .fetch_all(conn)
        .await?;
    if terms.is_empty() {
        return Ok(());
    }
    if candidates
        .iter()
        .any(|text| contains_filtered_term(text, &terms))
    {
        return Err(ApiError::unprocessable(
            "ugc.content_rejected",
            "This content cannot be posted.",
        ));
    }
    Ok(())
}

/// `guard.ensure_user_content_allowed`.
pub async fn ensure_user_content_allowed(
    conn: &mut PgConnection,
    user_id: &str,
    texts: &[&str],
) -> ApiResult<()> {
    require_terms_accepted(conn, user_id).await?;
    ensure_content_allowed(conn, texts).await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FilterTerm {
    pub id: String,
    pub term: String,
    pub created_at: NaiveDateTime,
}

pub async fn list_terms(conn: &mut PgConnection) -> ApiResult<Vec<FilterTerm>> {
    Ok(sqlx::query_as::<_, FilterTerm>(
        "SELECT id, term, created_at FROM ugc_filter_terms ORDER BY created_at DESC, id",
    )
    .fetch_all(conn)
    .await?)
}

/// `content_filter.add_term`.
pub async fn add_term(conn: &mut PgConnection, actor: &str, term: &str) -> ApiResult<FilterTerm> {
    let cleaned = term.trim();
    let normalized = normalize_text(cleaned);
    if cleaned.is_empty()
        || normalized.is_empty()
        || cleaned.chars().count() > FILTER_TERM_MAX_LENGTH
        || normalized.chars().count() > FILTER_TERM_MAX_LENGTH
    {
        return Err(ApiError::unprocessable(
            "ugc.filter_term_invalid",
            "Banned words must be 1-50 characters.",
        ));
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM ugc_filter_terms")
        .fetch_one(&mut *conn)
        .await?;
    if count >= FILTER_TERMS_MAX {
        return Err(ApiError::unprocessable(
            "ugc.filter_term_limit",
            "The banned word list is full.",
        ));
    }
    let row = FilterTerm {
        id: new_id(),
        term: cleaned.to_string(),
        created_at: now_utc(),
    };
    let inserted = sqlx::query(
        "INSERT INTO ugc_filter_terms (id, term, normalized, created_by_id, created_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&row.id)
    .bind(&row.term)
    .bind(&normalized)
    .bind(actor)
    .bind(row.created_at)
    .execute(&mut *conn)
    .await;
    match inserted {
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => {
            return Err(ApiError::conflict(
                "ugc.filter_term_duplicate",
                "This banned word is already registered.",
            ));
        }
        Err(error) => return Err(error.into()),
    }
    audit::record(
        conn,
        "ugc_filter.term_added",
        "ugc_filter_term",
        Some(&row.id),
        "DM·업무 금칙어를 더했다",
        actor,
        json!({"length": cleaned.chars().count()}),
    )
    .await?;
    Ok(row)
}

/// `content_filter.remove_term`.
pub async fn remove_term(conn: &mut PgConnection, actor: &str, term_id: &str) -> ApiResult<()> {
    let deleted = sqlx::query("DELETE FROM ugc_filter_terms WHERE id = $1")
        .bind(term_id)
        .execute(&mut *conn)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found(
            "ugc.filter_term_not_found",
            "Banned word not found.",
        ));
    }
    audit::record(
        conn,
        "ugc_filter.term_removed",
        "ugc_filter_term",
        Some(term_id),
        "DM·업무 금칙어를 뺐다",
        actor,
        json!({}),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    // tests/test_dm_safety.py (content filter cases)
    #[test]
    fn normalization_removes_separators_and_case() {
        assert_eq!(normalize_text("Ba d.W-o_r d"), "badword");
        assert_eq!(normalize_text("ＢＡＤ"), "bad");
        assert_eq!(normalize_text("나\u{200B}쁜·말"), "나쁜말");
        assert_eq!(normalize_text("㍿"), "株式会社");
    }

    #[test]
    fn filtered_terms_match_inside_normalized_text() {
        let terms = vec![normalize_text("나쁜말")];
        assert!(contains_filtered_term("이건 나 쁜 말 이다", &terms));
        assert!(contains_filtered_term("나*쁜*말", &terms));
        assert!(!contains_filtered_term("좋은 말", &terms));
        assert!(!contains_filtered_term("", &terms));
        assert!(!contains_filtered_term("anything", &[String::new()]));
    }
}
