//! Bounded directory reads for display and current original-user membership.
use crate::{CallError, Identity, Method, call_error};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DirectoryKind {
    #[default]
    Users,
    Groups,
    Memberships,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct DirectoryGroupRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryUser {
    pub id: String,
    pub name: String,
    pub primary_group: Option<DirectoryGroupRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DirectoryGroupSource {
    Hr,
    Local,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct DirectoryGroup {
    pub id: String,
    pub name: String,
    pub source: DirectoryGroupSource,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryPage {
    pub kind: DirectoryKind,
    pub subject_id: String,
    pub installation_id: String,
    pub environment: String,
    pub users: Vec<DirectoryUser>,
    pub groups: Vec<DirectoryGroup>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DirectoryQuery {
    pub kind: DirectoryKind,
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    /// Resolve selected stable references, at most 100 per request.
    pub ids: Option<Vec<String>>,
}

impl DirectoryQuery {
    pub(crate) fn suffix(&self) -> Result<String, CallError> {
        let q = self.q.as_deref().unwrap_or_default().trim();
        if q.chars().count() > 120
            || (self.kind == DirectoryKind::Memberships && !q.is_empty())
            || self.limit.is_some_and(|limit| !(1..=100).contains(&limit))
            || self.cursor.as_ref().is_some_and(String::is_empty)
            || self.ids.as_ref().is_some_and(|ids| {
                ids.is_empty()
                    || ids.len() > 100
                    || ids
                        .iter()
                        .any(|id| id.is_empty() || id.chars().count() > 256 || id.contains(','))
            })
        {
            return Err(call_error("invalid_directory_query"));
        }
        // This fixed URL only encodes query values. The actual core origin is
        // resolved by Identity::call and can never be supplied by this query.
        let mut url = reqwest::Url::parse("https://miy.invalid/platform/directory").unwrap();
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair(
                "kind",
                match self.kind {
                    DirectoryKind::Users => "users",
                    DirectoryKind::Groups => "groups",
                    DirectoryKind::Memberships => "memberships",
                },
            );
            if self.q.is_some() {
                pairs.append_pair("q", q);
            }
            if let Some(limit) = self.limit {
                pairs.append_pair("limit", &limit.to_string());
            }
            if let Some(cursor) = &self.cursor {
                pairs.append_pair("cursor", cursor);
            }
            if let Some(ids) = &self.ids {
                pairs.append_pair("ids", &ids.join(","));
            }
        }
        Ok(format!("platform/directory?{}", url.query().unwrap()))
    }
}

impl Identity {
    /// Rechecks original-user access and exact installation/release approval at
    /// Core on every call. A directory selection is not a permission decision.
    pub async fn directory(&self, query: &DirectoryQuery) -> Result<DirectoryPage, CallError> {
        let data = self.call(&query.suffix()?, Method::GET, None).await?;
        let page: DirectoryPage =
            serde_json::from_value(data).map_err(|_| call_error("invalid_directory_response"))?;
        if page.subject_id != self.user_id
            || page.kind != query.kind
            || page.installation_id != self.context.installation_id
            || Some(page.environment.as_str()) != self.context.environment.as_deref()
        {
            return Err(call_error("invalid_directory_response"));
        }
        Ok(page)
    }

    /// Current, exact membership only: no primary-group ancestors or nested
    /// groups. Call failure is an error, never a positive membership result.
    pub async fn is_group_member(&self, group_id: &str) -> Result<bool, CallError> {
        if !crate::valid_uuid(group_id) {
            return Err(call_error("invalid_directory_query"));
        }
        let page = self
            .directory(&DirectoryQuery {
                kind: DirectoryKind::Memberships,
                ids: Some(vec![group_id.to_owned()]),
                limit: Some(1),
                ..Default::default()
            })
            .await?;
        // Core answers in lowercase whatever case the caller holds.
        Ok(page
            .groups
            .iter()
            .any(|group| group.id.eq_ignore_ascii_case(group_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_values_cannot_change_the_route_or_add_query_fields() {
        let query = DirectoryQuery {
            q: Some("  name &kind=memberships/../secret  ".into()),
            cursor: Some("cursor&userId=other".into()),
            ids: Some(vec!["user&admin=true".into()]),
            ..Default::default()
        };
        let url = reqwest::Url::parse(&format!("https://miy.invalid/{}", query.suffix().unwrap()))
            .unwrap();
        assert_eq!(url.path(), "/platform/directory");
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(pairs.len(), 4);
        assert_eq!(pairs["kind"], "users");
        assert_eq!(pairs["q"], "name &kind=memberships/../secret");
        assert_eq!(pairs["cursor"], "cursor&userId=other");
        assert_eq!(pairs["ids"], "user&admin=true");
    }

    #[test]
    fn bounded_queries_reject_incomplete_or_ambiguous_filters() {
        let invalid = [
            DirectoryQuery {
                kind: DirectoryKind::Memberships,
                q: Some("search".into()),
                ..Default::default()
            },
            DirectoryQuery {
                limit: Some(0),
                ..Default::default()
            },
            DirectoryQuery {
                limit: Some(101),
                ..Default::default()
            },
            DirectoryQuery {
                q: Some("한".repeat(121)),
                ..Default::default()
            },
            DirectoryQuery {
                cursor: Some(String::new()),
                ..Default::default()
            },
            DirectoryQuery {
                ids: Some(vec![]),
                ..Default::default()
            },
            DirectoryQuery {
                ids: Some(vec!["a,b".into()]),
                ..Default::default()
            },
            DirectoryQuery {
                ids: Some(vec!["a".into(); 101]),
                ..Default::default()
            },
        ];
        for query in invalid {
            assert_eq!(query.suffix().unwrap_err().code, "invalid_directory_query");
        }
    }
}
