//! Name and metadata search.
//!
//! AI-DO `POST /files/search` ranked file contents with BM25, vectors or both
//! (domains/files/search.py) over its RAG index. Content and semantic search
//! are out of scope (COMMON-FEATURES-PLAN §2.2, M13), so drive keeps the
//! request and response shapes and answers the `keyword` strategy from the
//! app database: every word of the query must appear in the file name or its
//! metadata (owner name, folder path, content type). `semantic` and `hybrid`
//! are refused with 422 `files_search_strategy_unsupported`.
//!
//! The portal's integrated search (`platformEndpoints.search`, M12) matches
//! file names over the spaces the person may open.
use axum::{
    Extension, Json,
    extract::{Path, State},
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::{
    AppState,
    access::{self, SpaceAccess},
    error::{ApiError, ApiJson, ApiResult, check_length},
    people,
    time::{naive_iso, utc_iso},
    tree::{self, FileRow, Folder},
};

pub const MAX_RANKED_RESULTS: u32 = 100;

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
pub(crate) enum FileSearchStrategy {
    Keyword,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FileSearchRequest {
    /// Search text containing at least one non-whitespace character.
    #[schema(min_length = 1, max_length = 2000)]
    query: String,
    /// Searches file names and metadata. Legacy requests that omit this field
    /// default to `hybrid` and receive HTTP 422, as do explicit `semantic` and `hybrid` requests.
    #[serde(default = "default_strategy")]
    #[schema(value_type = FileSearchStrategy, required = true)]
    strategy: String,
    /// One-based page. The product of `page` and `page_size` must be at most 100.
    #[serde(default = "default_page")]
    #[schema(minimum = 1, maximum = 100, default = 1)]
    page: u32,
    #[serde(default = "default_page_size")]
    #[schema(minimum = 1, maximum = 50, default = 20)]
    page_size: u32,
    #[serde(default)]
    source_kind: Option<String>,
    /// Owner name (AI-DO: the source author).
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    department: Option<String>,
    /// A content type or its prefix (AI-DO: the source document type).
    #[serde(default)]
    document_type: Option<String>,
    /// Upload time range (AI-DO: the source authored time).
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    authored_from: Option<NaiveDateTime>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    authored_to: Option<NaiveDateTime>,
}

fn default_strategy() -> String {
    "hybrid".into()
}
fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}

#[derive(Serialize, Clone, Debug, PartialEq, utoipa::ToSchema)]
pub struct Highlight {
    pub start: usize,
    pub end: usize,
}

#[derive(Serialize, Clone, utoipa::ToSchema)]
pub struct Snippet {
    text: String,
    highlights: Vec<Highlight>,
}

#[derive(Serialize, Clone, utoipa::ToSchema)]
pub struct FileSearchHit {
    rank: u32,
    file_id: String,
    filename: String,
    title: String,
    #[schema(required = true)]
    folder_id: Option<String>,
    content_type: String,
    size_bytes: i64,
    score: f64,
    methods: Vec<&'static str>,
    snippet: Snippet,
    updated_at: String,
    source_kind: &'static str,
    #[schema(required = true)]
    author: Option<String>,
    #[schema(required = true)]
    authored_at: Option<String>,
    #[schema(required = true)]
    department: Option<String>,
    #[schema(required = true)]
    document_type: Option<String>,
    #[schema(required = true)]
    source_updated_at: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FileSearchResponse {
    query: String,
    #[schema(value_type = FileSearchStrategy)]
    strategy: &'static str,
    page: u32,
    page_size: u32,
    hits: Vec<FileSearchHit>,
    has_more: bool,
    max_ranked_results: u32,
    latency_ms: u64,
    #[schema(required = true)]
    trace_id: Option<String>,
}

fn normalize_filter(name: &str, value: &Option<String>, max: usize) -> ApiResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    check_length(name, value, 1, max)?;
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(ApiError::validation(format!(
            "{name}: Value error, metadata filters must not be blank"
        )));
    }
    Ok(Some(normalized))
}

pub fn tokens(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|t| !t.is_empty())
        .collect()
}

fn lower_char(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// UTF-16 ranges (the browser's string offsets) of each token in `text`,
/// case-insensitive, merged.
pub fn highlights(text: &str, tokens: &[String]) -> Vec<Highlight> {
    let chars: Vec<char> = text.chars().collect();
    let lowered: Vec<char> = chars.iter().map(|c| lower_char(*c)).collect();
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut total = 0;
    for c in &chars {
        offsets.push(total);
        total += c.len_utf16();
    }
    offsets.push(total);
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for token in tokens {
        let needle: Vec<char> = token.chars().map(lower_char).collect();
        if needle.is_empty() || needle.len() > lowered.len() {
            continue;
        }
        for start in 0..=lowered.len() - needle.len() {
            if lowered[start..start + needle.len()] == needle[..] {
                ranges.push((start, start + needle.len()));
            }
        }
    }
    ranges.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
        .into_iter()
        .map(|(start, end)| Highlight {
            start: offsets[start],
            end: offsets[end],
        })
        .collect()
}

fn folder_paths(folders: &[Folder]) -> HashMap<String, String> {
    let by_id: HashMap<&str, &Folder> = folders.iter().map(|f| (f.id.as_str(), f)).collect();
    folders
        .iter()
        .map(|folder| {
            let mut names = Vec::new();
            let mut seen = HashSet::new();
            let mut cursor = Some(folder);
            while let Some(current) = cursor {
                if !seen.insert(current.id.as_str()) {
                    break;
                }
                names.push(current.name.clone());
                cursor = current
                    .parent_id
                    .as_deref()
                    .and_then(|id| by_id.get(id).copied());
            }
            names.reverse();
            (folder.id.clone(), names.join(" / "))
        })
        .collect()
}

struct Candidate<'a> {
    file: &'a FileRow,
    score: f64,
    methods: Vec<&'static str>,
}

/// Every token must appear in the file name or its metadata. Ranked: the
/// whole query as the name, the name starting with it, all words in the name,
/// then metadata-only matches.
fn score<'a>(
    file: &'a FileRow,
    query: &str,
    tokens: &[String],
    folder_path: &str,
    owner_name: &str,
) -> Option<Candidate<'a>> {
    let name = file.filename.to_lowercase();
    let metadata = format!(
        "{} {} {}",
        owner_name.to_lowercase(),
        folder_path.to_lowercase(),
        file.content_type.to_lowercase()
    );
    let mut in_name = true;
    for token in tokens {
        let named = name.contains(token);
        if !named && !metadata.contains(token) {
            return None;
        }
        in_name &= named;
    }
    let whole = query.trim().to_lowercase();
    let stem = name
        .rsplit_once('.')
        .map_or(name.as_str(), |(stem, _)| stem);
    let (score, methods) = if name == whole || stem == whole {
        (3.0, vec!["filename"])
    } else if name.starts_with(&whole) {
        (2.0, vec!["filename"])
    } else if in_name {
        (1.0, vec!["filename"])
    } else {
        (0.5, vec!["filename", "metadata"])
    };
    Some(Candidate {
        file,
        score,
        methods,
    })
}

/// `POST /v1/spaces/{space_id}/files/search` (AI-DO `search_files`).
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/files/search", operation_id = "searchDriveFiles", tag = "Search",
    request_body = FileSearchRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 200, description = "Ranked results containing only readable files", body = FileSearchResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid input returns `validation_request_invalid`. Legacy requests that omit `strategy` or specify `semantic` or `hybrid` return `files_search_strategy_unsupported` when the other fields pass validation.", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn search_files(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<FileSearchRequest>,
) -> ApiResult<Json<FileSearchResponse>> {
    let started = std::time::Instant::now();
    check_length("query", &payload.query, 1, 2000)?;
    if payload.page < 1 || !(1..=50).contains(&payload.page_size) {
        return Err(ApiError::validation(
            "page must be at least 1 and page_size between 1 and 50",
        ));
    }
    if payload.page * payload.page_size > MAX_RANKED_RESULTS {
        return Err(ApiError::validation(format!(
            "page and page_size must stay within the first {MAX_RANKED_RESULTS} ranked results"
        )));
    }
    if let (Some(from), Some(to)) = (payload.authored_from, payload.authored_to)
        && from > to
    {
        return Err(ApiError::validation(
            "authored_from must not be after authored_to",
        ));
    }
    normalize_filter("source_kind", &payload.source_kind, 80)?;
    normalize_filter("department", &payload.department, 512)?;
    let author = normalize_filter("author", &payload.author, 512)?;
    let document_type = normalize_filter("document_type", &payload.document_type, 255)?;
    if payload.strategy != "keyword" {
        if matches!(payload.strategy.as_str(), "semantic" | "hybrid") {
            return Err(ApiError::unprocessable(
                "files.search_strategy_unsupported",
                "Only name and metadata search (keyword) is available.",
            ));
        }
        return Err(ApiError::validation(
            "strategy: Input should be 'keyword', 'semantic' or 'hybrid'",
        ));
    }
    let query_tokens = tokens(&payload.query);
    if query_tokens.is_empty() {
        return Err(ApiError::validation("query: must not be blank"));
    }
    let access = access::read_access(&state, &identity, &space_id).await?;
    let folders = tree::list_accessible_folders(&state.pool, &access).await?;
    let folder_ids: HashSet<String> = folders.iter().map(|f| f.id.clone()).collect();
    let files = tree::list_accessible_files(&state.pool, &access, &folder_ids).await?;
    let paths = folder_paths(&folders);
    // Department and source kind have no counterpart in drive metadata:
    // a filter on them matches nothing but "files".
    let excluded = payload
        .source_kind
        .as_deref()
        .is_some_and(|kind| kind.trim() != "files")
        || payload.department.is_some();
    let author_ids: Option<HashSet<String>> = match &author {
        Some(author) => Some(
            people::search_user_ids(&identity, author)
                .await
                .into_iter()
                .collect(),
        ),
        None => None,
    };
    let names = people::display_names(&identity, files.iter().map(|f| f.owner_id.clone())).await;
    let mut candidates: Vec<Candidate> = files
        .iter()
        .filter(|_| !excluded)
        .filter(|file| {
            author_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&file.owner_id))
        })
        .filter(|file| {
            document_type.as_ref().is_none_or(|kind| {
                file.content_type
                    .to_lowercase()
                    .starts_with(&kind.to_lowercase())
            })
        })
        .filter(|file| {
            payload
                .authored_from
                .is_none_or(|from| file.created_at >= from)
        })
        .filter(|file| payload.authored_to.is_none_or(|to| file.created_at <= to))
        .filter_map(|file| {
            let path = file
                .folder_id
                .as_ref()
                .and_then(|id| paths.get(id))
                .map_or("", String::as_str);
            let owner = names.get(&file.owner_id).map_or("", String::as_str);
            score(file, &payload.query, &query_tokens, path, owner)
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.file.updated_at.cmp(&a.file.updated_at))
            .then(a.file.id.cmp(&b.file.id))
    });
    candidates.truncate(MAX_RANKED_RESULTS as usize);
    let start = ((payload.page - 1) * payload.page_size) as usize;
    let end = start + payload.page_size as usize;
    let has_more = candidates.len() > end;
    let hits = candidates
        .iter()
        .enumerate()
        .skip(start)
        .take(payload.page_size as usize)
        .map(|(index, candidate)| {
            let file = candidate.file;
            FileSearchHit {
                rank: index as u32 + 1,
                file_id: file.id.clone(),
                filename: file.filename.clone(),
                title: file.filename.clone(),
                folder_id: file.folder_id.clone(),
                content_type: file.content_type.clone(),
                size_bytes: file.size_bytes,
                score: candidate.score,
                methods: candidate.methods.clone(),
                snippet: Snippet {
                    text: file.filename.clone(),
                    highlights: highlights(&file.filename, &query_tokens),
                },
                updated_at: naive_iso(file.updated_at),
                source_kind: "files",
                author: names.get(&file.owner_id).cloned(),
                authored_at: Some(naive_iso(file.created_at)),
                department: None,
                document_type: Some(file.content_type.clone()),
                source_updated_at: None,
            }
        })
        .collect();
    Ok(Json(FileSearchResponse {
        query: payload.query.clone(),
        strategy: "keyword",
        page: payload.page,
        page_size: payload.page_size,
        hits,
        has_more,
        max_ranked_results: MAX_RANKED_RESULTS,
        latency_ms: started.elapsed().as_millis() as u64,
        trace_id: None,
    }))
}

// ------------------------------------------------- portal integrated search

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSearchRequest {
    query: String,
    limit: u32,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PlatformSearchItem {
    title: String,
    snippet: String,
    path: String,
    at: String,
    kind: &'static str,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PlatformSearchResponse {
    items: Vec<PlatformSearchItem>,
}

pub fn platform_only(identity: &Identity) -> ApiResult<()> {
    if !identity.context.platform {
        return Err(ApiError::forbidden(
            "files.platform_only",
            "Only the platform calls this endpoint.",
        ));
    }
    Ok(())
}

/// Escape `%`, `_` and `\` for a `LIKE` pattern.
pub fn like_pattern(token: &str) -> String {
    let mut out = String::with_capacity(token.len() + 2);
    out.push('%');
    for c in token.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// `POST /v1/search` (platformEndpoints.search): file names the person may
/// open now. The call cannot ask the directory; the person's current groups
/// come signed with it (`identity.context.groups`).
#[utoipa::path(
    post, path = "/v1/search", operation_id = "searchDriveForPlatform", tag = "Platform callbacks",
    request_body = PlatformSearchRequest,
    responses(
        (status = 200, description = "Readable links for platform search", body = PlatformSearchResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 400, description = "Malformed request", body = String, content_type = "text/plain"),
        (status = 403, description = "Only the platform may call this endpoint", body = crate::error::ErrorResponse),
        (status = 415, description = "Unsupported content type", body = String, content_type = "text/plain"),
        (status = 422, description = "Invalid or unsupported request", body = String, content_type = "text/plain"),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn platform_search(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Json(payload): Json<PlatformSearchRequest>,
) -> ApiResult<Json<PlatformSearchResponse>> {
    platform_only(&identity)?;
    let limit = payload.limit.clamp(1, 10) as usize;
    let query_tokens = tokens(&payload.query);
    if query_tokens.is_empty() {
        return Ok(Json(PlatformSearchResponse { items: Vec::new() }));
    }
    let english = payload.locale.as_deref() == Some("en-US");
    let roles =
        access::roles_by_space(&state.pool, &identity, &access::platform_groups(&identity)).await?;
    let space_ids: Vec<String> = roles.iter().map(|(id, _)| id.clone()).collect();
    let patterns: Vec<String> = query_tokens.iter().map(|t| like_pattern(t)).collect();
    let sql = format!(
        "{} WHERE f.workspace_id = ANY($1) AND f.deleted_at IS NULL AND f.current_version_id IS NOT NULL \
         AND (SELECT bool_and(f.filename ILIKE p ESCAPE '\\') FROM unnest($2::text[]) AS p) \
         ORDER BY f.updated_at DESC, f.id LIMIT 500",
        tree::FILE_SELECT
    );
    let candidates = sqlx::query_as::<_, FileRow>(&sql)
        .bind(&space_ids)
        .bind(&patterns)
        .fetch_all(&state.pool)
        .await?;
    let mut items = Vec::new();
    let mut spaces: HashMap<String, (SpaceAccess, HashSet<String>, HashMap<String, String>)> =
        HashMap::new();
    for file in &candidates {
        if items.len() >= limit {
            break;
        }
        if !spaces.contains_key(&file.workspace_id) {
            let Some(access) =
                access::platform_access(&state.pool, &identity, &file.workspace_id).await?
            else {
                continue;
            };
            let folders = tree::list_accessible_folders(&state.pool, &access).await?;
            let ids = folders.iter().map(|f| f.id.clone()).collect();
            let paths = folder_paths(&folders);
            spaces.insert(file.workspace_id.clone(), (access, ids, paths));
        }
        let (access, folder_ids, paths) = &spaces[&file.workspace_id];
        if !tree::record_visible(&file.visibility, &file.owner_id, access)
            || file
                .folder_id
                .as_ref()
                .is_some_and(|id| !folder_ids.contains(id))
        {
            continue;
        }
        let location = file
            .folder_id
            .as_ref()
            .and_then(|id| paths.get(id))
            .cloned()
            .unwrap_or_else(|| {
                if english {
                    "All files".into()
                } else {
                    "전체 파일".into()
                }
            });
        let path = match &file.folder_id {
            Some(folder) => format!("/spaces/{}?folder={folder}", file.workspace_id),
            None => format!("/spaces/{}", file.workspace_id),
        };
        items.push(PlatformSearchItem {
            title: file.filename.clone(),
            snippet: format!("{} · {location}", access.space.name),
            path,
            at: utc_iso(file.updated_at),
            kind: if english { "File" } else { "파일" },
        });
    }
    Ok(Json(PlatformSearchResponse { items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_are_browser_string_offsets() {
        let ranges = highlights("Plan 계획서 PLAN.pdf", &tokens("plan 계획"));
        assert_eq!(
            ranges,
            vec![
                Highlight { start: 0, end: 4 },
                Highlight { start: 5, end: 7 },
                Highlight { start: 9, end: 13 },
            ]
        );
        // An emoji takes two UTF-16 units.
        let ranges = highlights("😀report", &tokens("report"));
        assert_eq!(ranges, vec![Highlight { start: 2, end: 8 }]);
        assert!(highlights("abc", &tokens("zzzz")).is_empty());
    }

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(like_pattern("50%_a\\b"), "%50\\%\\_a\\\\b%");
    }
}
