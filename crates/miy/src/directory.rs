//! Bounded app-facing directory data. Every caller must first validate its current
//! installation/release permission and original user. These DTOs are not admin APIs.
use crate::{
    State,
    api_contract::{
        DirectoryGroup, DirectoryGroupRef, DirectoryKind, DirectoryPage, DirectoryUser, GroupSource,
    },
    http::{ApiError, ApiResult},
};
use axum::{extract::Query, http::Uri};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

const DEFAULT_LIMIT: u16 = 50;
const MAX_LIMIT: u16 = 100;
const MAX_QUERY_CHARS: usize = 254;
const MAX_ID_CHARS: usize = 256;
// A lowercased display name may expand in some Unicode case mappings.
const MAX_CURSOR_NAME_CHARS: usize = 512;
const MAX_CURSOR_BYTES: usize = 4096;

/// App directory query. `ids` is a comma-separated candidate id allow-list, not
/// an instruction to grant access. Memberships always describe the original user.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectoryQuery {
    #[serde(default)]
    pub kind: DirectoryKind,
    /// Literal, case-insensitive name search. Not supported for memberships.
    pub q: Option<String>,
    /// Between 1 and 100, defaults to 50.
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    /// At most 100 ids; group/membership ids must be UUIDs. Duplicates are removed.
    pub ids: Option<String>,
}

#[derive(Debug)]
struct Filter {
    kind: DirectoryKind,
    q: String,
    limit: u16,
    ids: Option<Vec<String>>,
    cursor: Option<Cursor>,
    context: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    v: u8,
    context: String,
    name: String,
    id: String,
}

impl DirectoryQuery {
    pub fn from_uri(uri: &Uri) -> ApiResult<Self> {
        Query::<Self>::try_from_uri(uri)
            .map(|query| query.0)
            .map_err(|_| ApiError::bad("invalid_directory_query"))
    }

    fn validate(self, subject: &str, installation: Uuid, release: Uuid) -> ApiResult<Filter> {
        let q = self.q.unwrap_or_default().trim().to_lowercase();
        if q.chars().count() > MAX_QUERY_CHARS || q.chars().any(char::is_control) {
            return Err(ApiError::bad("invalid_directory_query"));
        }
        if self.kind == DirectoryKind::Memberships && !q.is_empty() {
            return Err(ApiError::bad("directory_memberships_search_not_supported"));
        }
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(ApiError::bad("invalid_directory_limit"));
        }
        let ids = self
            .ids
            .map(|ids| {
                let mut ids: Vec<String> = ids.split(',').map(str::to_owned).collect();
                if ids.is_empty() || ids.len() > usize::from(MAX_LIMIT) {
                    return Err(ApiError::bad("invalid_directory_ids"));
                }
                for id in &mut ids {
                    if id.is_empty()
                        || id.trim() != id
                        || id.chars().count() > MAX_ID_CHARS
                        || id.chars().any(char::is_control)
                    {
                        return Err(ApiError::bad("invalid_directory_ids"));
                    }
                    if self.kind != DirectoryKind::Users {
                        *id = Uuid::parse_str(id)
                            .map_err(|_| ApiError::bad("invalid_directory_ids"))?
                            .to_string();
                    }
                }
                ids.sort();
                ids.dedup();
                Ok(ids)
            })
            .transpose()?;
        // Only a digest of the request scope is carried in the cursor. It has no
        // authority: current source admission and directory approval are rechecked
        // for every page. A forged cursor can only skip rows in that same query.
        let context = crate::contract::hash(&serde_json::to_vec(&(
            1,
            subject,
            installation,
            release,
            self.kind,
            &q,
            &ids,
        ))?);
        let cursor = self
            .cursor
            .map(|encoded| {
                if encoded.is_empty() || encoded.len() > MAX_CURSOR_BYTES {
                    return Err(ApiError::bad("invalid_directory_cursor"));
                }
                let bytes = URL_SAFE_NO_PAD
                    .decode(encoded)
                    .map_err(|_| ApiError::bad("invalid_directory_cursor"))?;
                let cursor: Cursor = serde_json::from_slice(&bytes)
                    .map_err(|_| ApiError::bad("invalid_directory_cursor"))?;
                if cursor.v != 1
                    || cursor.context != context
                    || cursor.name.chars().count() > MAX_CURSOR_NAME_CHARS
                    || cursor.id.is_empty()
                    || cursor.id.chars().count() > MAX_ID_CHARS
                    || cursor.id.chars().any(char::is_control)
                    || (self.kind != DirectoryKind::Users && Uuid::parse_str(&cursor.id).is_err())
                {
                    return Err(ApiError::bad("invalid_directory_cursor"));
                }
                Ok(cursor)
            })
            .transpose()?;
        Ok(Filter {
            kind: self.kind,
            q,
            limit,
            ids,
            cursor,
            context,
        })
    }
}

fn next_cursor(filter: &Filter, name: String, id: String) -> ApiResult<String> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&Cursor {
        v: 1,
        context: filter.context.clone(),
        name,
        id,
    })?))
}

pub async fn read(
    state: &State,
    subject: &str,
    installation: Uuid,
    release: Uuid,
    environment: crate::runtime_contract::Environment,
    query: DirectoryQuery,
) -> ApiResult<DirectoryPage> {
    let filter = query.validate(subject, installation, release)?;
    let cursor_name = filter.cursor.as_ref().map(|cursor| cursor.name.as_str());
    let cursor_id = filter.cursor.as_ref().map(|cursor| cursor.id.as_str());
    let mut page = DirectoryPage {
        kind: filter.kind,
        subject_id: subject.to_owned(),
        installation_id: installation,
        environment,
        users: Vec::new(),
        groups: Vec::new(),
        next_cursor: None,
    };
    match filter.kind {
        DirectoryKind::Users => {
            // strpos performs literal matching, so '%' and '_' never widen a
            // user's search into a SQL wildcard. Name bounds also bound response
            // and cursor size for names imported from an external identity source.
            let lookup = !filter.q.is_empty()
                && filter
                    .q
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"@._+-".contains(&b));
            let aliases = if lookup && let Some(provider) = state.auth.accounts.as_ref() {
                provider.search_alias(&filter.q).await
            } else {
                Ok(Vec::new())
            };
            let mut rows = sqlx::query(
                "SELECT u.id,left(u.name,256) AS name,lower(left(u.name,256)) AS sort_name, \
                 g.id AS primary_group_id,g.name AS primary_group_name \
                 FROM users u LEFT JOIN groups g ON g.id=u.primary_group_id AND g.active AND g.source='hr' \
                 WHERE u.enabled AND ($1='' OR strpos(lower(u.name),$1)>0 OR strpos(lower(g.name),$1)>0 OR u.id=ANY($6::text[]) OR lower(u.id)=$1) \
                 AND ($2::text[] IS NULL OR u.id=ANY($2)) \
                 AND ($3::text IS NULL OR (lower(left(u.name,256)) COLLATE \"C\",u.id COLLATE \"C\") \
                      > ($3 COLLATE \"C\",$4 COLLATE \"C\")) \
                 ORDER BY lower(left(u.name,256)) COLLATE \"C\",u.id COLLATE \"C\" LIMIT $5",
            )
            .bind(&filter.q)
            .bind(&filter.ids)
            .bind(cursor_name)
            .bind(cursor_id)
            .bind(i64::from(filter.limit) + 1)
            .bind(aliases.as_ref().map(Vec::as_slice).unwrap_or_default())
            .fetch_all(&state.pool)
            .await?;
            if rows.is_empty() {
                aliases?;
            }
            if rows.len() > usize::from(filter.limit) {
                rows.pop();
                if let Some(last) = rows.last() {
                    page.next_cursor =
                        Some(next_cursor(&filter, last.get("sort_name"), last.get("id"))?);
                }
            }
            page.users = rows
                .into_iter()
                .map(|row| DirectoryUser {
                    id: row.get("id"),
                    name: row.get("name"),
                    primary_group: row.get::<Option<Uuid>, _>("primary_group_id").map(|id| {
                        DirectoryGroupRef {
                            id,
                            name: row.get("primary_group_name"),
                        }
                    }),
                })
                .collect();
        }
        DirectoryKind::Groups | DirectoryKind::Memberships => {
            let mut rows = sqlx::query(
                "SELECT g.id,g.name,g.source,lower(g.name) AS sort_name FROM groups g \
                 WHERE g.active AND ($1='' OR strpos(lower(g.name),$1)>0) \
                 AND ($2::text[] IS NULL OR g.id::text=ANY($2)) \
                 AND ($3::text IS NULL OR (lower(g.name) COLLATE \"C\",g.id::text COLLATE \"C\") \
                      > ($3 COLLATE \"C\",$4 COLLATE \"C\")) \
                 AND (NOT $5::bool OR EXISTS(SELECT 1 FROM users u WHERE u.id=$6 AND u.enabled AND ( \
                     (g.source='hr' AND u.primary_group_id=g.id) OR \
                     (g.source='local' AND EXISTS(SELECT 1 FROM group_members m WHERE m.group_id=g.id AND m.user_id=u.id))))) \
                 ORDER BY lower(g.name) COLLATE \"C\",g.id::text COLLATE \"C\" LIMIT $7",
            )
            .bind(&filter.q)
            .bind(&filter.ids)
            .bind(cursor_name)
            .bind(cursor_id)
            .bind(filter.kind == DirectoryKind::Memberships)
            .bind(subject)
            .bind(i64::from(filter.limit) + 1)
            .fetch_all(&state.pool)
            .await?;
            if rows.len() > usize::from(filter.limit) {
                rows.pop();
                if let Some(last) = rows.last() {
                    page.next_cursor = Some(next_cursor(
                        &filter,
                        last.get("sort_name"),
                        last.get::<Uuid, _>("id").to_string(),
                    )?);
                }
            }
            page.groups = rows
                .into_iter()
                .map(|row| DirectoryGroup {
                    id: row.get("id"),
                    name: row.get("name"),
                    source: if row.get::<String, _>("source") == "hr" {
                        GroupSource::Hr
                    } else {
                        GroupSource::Local
                    },
                })
                .collect();
        }
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(query: &str) -> ApiResult<DirectoryQuery> {
        DirectoryQuery::from_uri(&format!("/platform/directory?{query}").parse().unwrap())
    }

    fn checked(query: &str) -> ApiResult<Filter> {
        parsed(query)?.validate("original-user", Uuid::nil(), Uuid::nil())
    }

    #[test]
    fn query_is_bounded_and_does_not_accept_subject_override() {
        let default = checked("").unwrap();
        assert_eq!(default.kind, DirectoryKind::Users);
        assert_eq!(default.limit, 50);
        assert_eq!(checked("q=%20%25_%20").unwrap().q, "%_");
        for query in [
            "kind=admin",
            "kind=users&kind=groups",
            "userId=other",
            "subjectId=other",
            "limit=0",
            "limit=101",
            "limit=-1",
            "limit=invalid",
            "q=%00",
            "ids=",
            "ids=a,,b",
            "kind=groups&ids=not-a-uuid",
            "kind=memberships&q=someone",
            "cursor=",
            "cursor=not-json",
        ] {
            assert!(checked(query).is_err(), "unexpected valid query: {query}");
        }
        assert!(checked(&format!("q={}", "a".repeat(MAX_QUERY_CHARS + 1))).is_err());
        assert!(checked(&format!("ids={}", vec!["a"; 101].join(","))).is_err());
        assert!(checked(&format!("ids={}", "a".repeat(257))).is_err());
        assert_eq!(
            checked("ids=b,a,b").unwrap().ids,
            Some(vec!["a".into(), "b".into()])
        );
    }

    #[test]
    fn cursors_cannot_move_between_subjects_releases_or_filters() {
        let query = parsed("q=search&ids=b,a").unwrap();
        let filter = query
            .clone()
            .validate("user", Uuid::nil(), Uuid::nil())
            .unwrap();
        let cursor = next_cursor(&filter, "same name".into(), "a".into()).unwrap();
        let query = DirectoryQuery {
            cursor: Some(cursor),
            ..query
        };
        assert!(
            query
                .clone()
                .validate("user", Uuid::nil(), Uuid::nil())
                .is_ok()
        );
        assert!(
            query
                .clone()
                .validate("other", Uuid::nil(), Uuid::nil())
                .is_err()
        );
        assert!(
            query
                .clone()
                .validate("user", Uuid::new_v4(), Uuid::nil())
                .is_err()
        );
        assert!(
            query
                .clone()
                .validate("user", Uuid::nil(), Uuid::new_v4())
                .is_err()
        );
        assert!(
            DirectoryQuery {
                q: Some("other".into()),
                ..query.clone()
            }
            .validate("user", Uuid::nil(), Uuid::nil())
            .is_err()
        );
        assert!(
            DirectoryQuery {
                ids: Some("a".into()),
                ..query
            }
            .validate("user", Uuid::nil(), Uuid::nil())
            .is_err()
        );
    }
}
