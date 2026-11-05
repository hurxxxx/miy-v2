//! App-level authorization in one place.
//!
//! Every decision is a pure function over [`Facts`] loaded once for a principal, so a
//! detail check, list visibility and the `canOpen`/`canManage`/`canPromote` display
//! values share one definition. Handlers name an [`AppAction`]; they do not choose
//! among check functions. Administrative endpoints keep `domain::admin` and
//! `domain::admin_locked`; queued work keeps its own recheck through
//! [`queued_manager`] and `delegations::check_queued`.
use crate::{
    State,
    auth::Principal,
    delegations::{self, Action},
    http::{ApiError, ApiResult},
};
use axum::http::StatusCode;
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};

/// Every installation, job and grant uses one of these environments (database checks).
pub const ENVIRONMENTS: [&str; 2] = ["dev", "prod"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppAction {
    /// Open and use an installation. The company switch stops this for everyone,
    /// administrators included, except developers in dev so the app can be repaired.
    Use,
    /// Change development state (releases, installations). Prod is administrator-only;
    /// delegated credentials never write here.
    Develop,
    /// Runtime and operations management under the environment's management grant
    /// (`develop` in dev, `deploy` in prod). Delegated credentials need this action in
    /// their scope. The company switch blocks use, not management.
    Manage(Action),
    /// Follow runtime jobs: management read, or otherwise use.
    FollowJob,
    /// Submit a runtime request. The receipt is an observable job, so the action and
    /// [`AppAction::FollowJob`] must both hold (a delegation needs the action and `read`).
    Request(Action),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denial {
    /// The user no longer exists.
    Unauthenticated,
    UserDisabled,
    DelegationInactive,
    DelegationScope,
    AppDisabled,
    Forbidden,
}
impl From<Denial> for ApiError {
    fn from(denial: Denial) -> Self {
        let code = match denial {
            Denial::Unauthenticated => return ApiError::unauthorized(),
            Denial::Forbidden => return ApiError::forbidden(),
            Denial::UserDisabled => "user_disabled",
            Denial::DelegationInactive => "delegation_expired_or_revoked",
            Denial::DelegationScope => "delegation_scope_denied",
            Denial::AppDisabled => "app_disabled",
        };
        ApiError {
            status: StatusCode::FORBIDDEN,
            code: code.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grants {
    pub use_: bool,
    pub develop: bool,
    pub deploy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct User {
    pub enabled: bool,
    pub admin: bool,
}

#[derive(Clone, Debug)]
pub enum Delegation {
    None,
    Active(delegations::Scope),
    /// Expired, revoked, without a live issued session, or its user is disabled.
    Inactive,
}

/// Display values that mirror the decisions a request would get.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flags {
    /// [`AppAction::Use`] on this installation.
    pub can_open: bool,
    /// [`AppAction::Manage`] with `read` on this installation: runtime management
    /// information (acceptance records, job details) under this credential.
    pub can_manage: bool,
    /// [`AppAction::Request`] with `promote` on the app's prod environment: the check a
    /// promotion request gets (a delegation also needs `read` there).
    pub can_promote: bool,
}

/// What the decisions read about one principal. Loading keeps the rows the principal
/// can reach; decisions never query.
#[derive(Clone, Debug)]
pub struct Facts {
    pub user: Option<User>,
    /// The credential carries `miy.dev`.
    pub dev_scope: bool,
    pub delegation: Delegation,
    pub grants: HashMap<(String, String), Grants>,
    /// Apps stopped by the company switch.
    pub disabled: HashSet<String>,
}

impl Facts {
    /// Facts for `p`, limited to `app` when given; `None` loads every app (lists).
    pub async fn load(pool: &PgPool, p: &Principal, app: Option<&str>) -> ApiResult<Self> {
        let delegation = match p.delegation_id {
            None => Delegation::None,
            Some(id) => match delegations::active(pool, &p.id, id).await {
                Ok(scope) => Delegation::Active(scope),
                Err(error) if error.status == StatusCode::FORBIDDEN => Delegation::Inactive,
                Err(error) => return Err(error),
            },
        };
        let mut facts = Self::actor(pool, &p.id, app).await?;
        facts.dev_scope = p.scopes.iter().any(|scope| scope == "miy.dev");
        facts.delegation = delegation;
        Ok(facts)
    }

    /// Facts for a stored actor without a credential (queued work): no scope, no delegation.
    pub async fn actor(pool: &PgPool, actor: &str, app: Option<&str>) -> Result<Self, sqlx::Error> {
        let user = sqlx::query("SELECT enabled,admin FROM users WHERE id=$1")
            .bind(actor)
            .fetch_optional(pool)
            .await?
            .map(|row| User {
                enabled: row.get("enabled"),
                admin: row.get("admin"),
            });
        let mut grants: HashMap<(String, String), Grants> = HashMap::new();
        for row in sqlx::query("SELECT app_id,environment,permission FROM effective_grants WHERE user_id=$1 AND ($2::text IS NULL OR app_id=$2)")
            .bind(actor)
            .bind(app)
            .fetch_all(pool)
            .await?
        {
            let entry = grants
                .entry((row.get("app_id"), row.get("environment")))
                .or_default();
            match row.get::<String, _>("permission").as_str() {
                "use" => entry.use_ = true,
                "develop" => entry.develop = true,
                "deploy" => entry.deploy = true,
                _ => {}
            }
        }
        let disabled = sqlx::query_scalar(
            "SELECT app_id FROM app_controls WHERE disabled AND ($1::text IS NULL OR app_id=$1)",
        )
        .bind(app)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        Ok(Self {
            user,
            dev_scope: false,
            delegation: Delegation::None,
            grants,
            disabled,
        })
    }

    fn grants(&self, app: &str, env: &str) -> Grants {
        self.grants
            .get(&(app.to_owned(), env.to_owned()))
            .copied()
            .unwrap_or_default()
    }

    fn enabled_user(&self) -> Option<User> {
        self.user.filter(|user| user.enabled)
    }

    /// The account is active (the request middleware's check, repeated per decision).
    fn active(&self) -> Result<User, Denial> {
        match self.user {
            None => Err(Denial::Unauthenticated),
            Some(user) if !user.enabled => Err(Denial::UserDisabled),
            Some(user) => Ok(user),
        }
    }

    /// The environment's management grant: administrators, `develop` in dev, `deploy` in prod.
    pub fn manages(&self, app: &str, env: &str) -> bool {
        self.enabled_user().is_some_and(|user| {
            let grants = self.grants(app, env);
            user.admin
                || if env == "prod" {
                    grants.deploy
                } else {
                    grants.develop
                }
        })
    }

    /// The user's own right behind a delegated action: the management grant for
    /// management actions, any grant for reads.
    pub fn delegable(&self, app: &str, env: &str, action: Action) -> bool {
        let grants = self.grants(app, env);
        self.enabled_user().is_some_and(|user| {
            user.admin
                || if action.management() {
                    self.manages(app, env)
                } else {
                    grants.use_ || grants.develop || grants.deploy
                }
        })
    }

    /// A delegated credential may act only inside its app, environment and actions.
    pub fn delegated(&self, app: &str, env: &str, action: Action) -> Result<(), Denial> {
        match &self.delegation {
            Delegation::None => Ok(()),
            Delegation::Inactive => Err(Denial::DelegationInactive),
            Delegation::Active(scope) => {
                if scope.allows(app, env, action) && self.delegable(app, env, action) {
                    Ok(())
                } else {
                    Err(Denial::DelegationScope)
                }
            }
        }
    }

    /// The app is listed for this user: administrators, or any grant in any environment.
    /// The company switch does not hide it.
    pub fn sees_app(&self, app: &str) -> bool {
        self.enabled_user().is_some_and(|user| {
            user.admin
                || ENVIRONMENTS.iter().any(|env| {
                    let grants = self.grants(app, env);
                    grants.use_ || grants.develop || grants.deploy
                })
        })
    }

    pub fn decide(&self, app: &str, env: &str, action: AppAction) -> Result<(), Denial> {
        match action {
            AppAction::Use => {
                let user = self.active()?;
                self.delegated(app, env, Action::Read)?;
                let grants = self.grants(app, env);
                let develop = user.admin || grants.develop;
                if self.disabled.contains(app) {
                    return if env == "dev" && develop {
                        Ok(())
                    } else {
                        Err(Denial::AppDisabled)
                    };
                }
                if develop || grants.use_ || grants.deploy {
                    Ok(())
                } else {
                    Err(Denial::Forbidden)
                }
            }
            AppAction::Develop => {
                let user = self.active()?;
                if !matches!(self.delegation, Delegation::None) {
                    return Err(Denial::DelegationScope);
                }
                let develop = user.admin || self.grants(app, env).develop;
                if self.dev_scope && develop && (user.admin || env != "prod") {
                    Ok(())
                } else {
                    Err(Denial::Forbidden)
                }
            }
            AppAction::Manage(action) => {
                self.delegated(app, env, action)?;
                if self.dev_scope && self.manages(app, env) {
                    Ok(())
                } else {
                    Err(Denial::Forbidden)
                }
            }
            AppAction::FollowJob => self
                .decide(app, env, AppAction::Manage(Action::Read))
                .or_else(|_| self.decide(app, env, AppAction::Use)),
            AppAction::Request(action) => {
                self.decide(app, env, AppAction::Manage(action))?;
                self.decide(app, env, AppAction::FollowJob)
            }
        }
    }

    pub fn flags(&self, app: &str, env: &str) -> Flags {
        Flags {
            can_open: self.decide(app, env, AppAction::Use).is_ok(),
            can_manage: self
                .decide(app, env, AppAction::Manage(Action::Read))
                .is_ok(),
            can_promote: self
                .decide(app, "prod", AppAction::Request(Action::Promote))
                .is_ok(),
        }
    }

    /// `(app, environment)` pairs among `apps` where `action` is allowed, as parallel
    /// arrays for `unnest($1::text[],$2::text[])` list filters.
    pub fn allowed_pairs(&self, apps: &[String], action: AppAction) -> (Vec<String>, Vec<String>) {
        apps.iter()
            .flat_map(|app| ENVIRONMENTS.iter().map(move |env| (app, *env)))
            .filter(|(app, env)| self.decide(app, env, action).is_ok())
            .map(|(app, env)| (app.clone(), env.to_owned()))
            .unzip()
    }
}

/// The single entry for request handlers: load the principal's facts for `app` and decide.
pub async fn require(
    state: &State,
    p: &Principal,
    app: &str,
    env: &str,
    action: AppAction,
) -> ApiResult<()> {
    Ok(Facts::load(&state.pool, p, Some(app))
        .await?
        .decide(app, env, action)?)
}

/// Queued work rechecks the stored actor's management grant; the delegation, if any,
/// is rechecked separately by `delegations::check_queued`.
pub async fn queued_manager(
    pool: &PgPool,
    actor: &str,
    app: &str,
    env: &str,
) -> Result<bool, sqlx::Error> {
    Ok(Facts::actor(pool, actor, Some(app))
        .await?
        .manages(app, env))
}

#[cfg(test)]
mod tests {
    //! The decisions must reproduce the checks they replaced. `legacy` transcribes
    //! the pre-policy functions (`domain::authorize`/`access_decision`, `can_manage`,
    //! `manage_action`, `delegations::check_id`/`granted`, `runtime::authorize_job`)
    //! and every combination of facts is compared, error codes included.
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    mod legacy {
        use super::*;
        pub struct Case<'a> {
            pub facts: &'a Facts,
            pub app: &'a str,
            pub env: &'a str,
        }
        fn grant(c: &Case, permission: &str) -> bool {
            let g = c.facts.grants(c.app, c.env);
            match permission {
                "use" => g.use_,
                "develop" => g.develop,
                "deploy" => g.deploy,
                _ => unreachable!(),
            }
        }
        // auth_sessions::ensure_active
        fn ensure_active(c: &Case) -> Result<(), Denial> {
            match c.facts.user {
                Some(u) if u.enabled => Ok(()),
                Some(_) => Err(Denial::UserDisabled),
                None => Err(Denial::Unauthenticated),
            }
        }
        // delegations::granted (SQL: users enabled AND (admin OR grant ...))
        fn granted(c: &Case, action: Action) -> bool {
            let Some(u) = c.facts.user else { return false };
            u.enabled
                && (u.admin
                    || if action.management() {
                        grant(c, if c.env == "prod" { "deploy" } else { "develop" })
                    } else {
                        grant(c, "use") || grant(c, "develop") || grant(c, "deploy")
                    })
        }
        // delegations::check -> check_id
        fn check(c: &Case, action: Action) -> Result<(), Denial> {
            match &c.facts.delegation {
                Delegation::None => Ok(()),
                Delegation::Inactive => Err(Denial::DelegationInactive),
                Delegation::Active(scope) => {
                    if !scope.allows(c.app, c.env, action) || !granted(c, action) {
                        Err(Denial::DelegationScope)
                    } else {
                        Ok(())
                    }
                }
            }
        }
        // domain::can_manage
        pub fn can_manage(c: &Case) -> bool {
            let Some(u) = c.facts.user else { return false };
            u.enabled && (u.admin || grant(c, if c.env == "prod" { "deploy" } else { "develop" }))
        }
        // domain::manage_action
        pub fn manage_action(c: &Case, action: Action) -> Result<(), Denial> {
            check(c, action)?;
            if !c.facts.dev_scope || !can_manage(c) {
                return Err(Denial::Forbidden);
            }
            Ok(())
        }
        // domain::authorize + access_decision
        pub fn authorize(c: &Case, write: bool) -> Result<(), Denial> {
            ensure_active(c)?;
            if write {
                if !matches!(c.facts.delegation, Delegation::None) {
                    return Err(Denial::DelegationScope);
                }
            } else {
                check(c, Action::Read)?;
            }
            if write && !c.facts.dev_scope {
                return Err(Denial::Forbidden);
            }
            let u = c
                .facts
                .user
                .filter(|u| u.enabled)
                .ok_or(Denial::Forbidden)?;
            let admin = u.admin;
            let develop = admin || grant(c, "develop");
            let other = grant(c, "use") || grant(c, "deploy");
            let disabled = c.facts.disabled.contains(c.app);
            if write {
                return if develop && (admin || c.env != "prod") {
                    Ok(())
                } else {
                    Err(Denial::Forbidden)
                };
            }
            if disabled {
                return if c.env == "dev" && develop {
                    Ok(())
                } else {
                    Err(Denial::AppDisabled)
                };
            }
            if develop || other {
                Ok(())
            } else {
                Err(Denial::Forbidden)
            }
        }
        // runtime::authorize_job
        pub fn authorize_job(c: &Case) -> Result<(), Denial> {
            if manage_action(c, Action::Read).is_err() {
                authorize(c, false)?;
            }
            Ok(())
        }
        // runtime::authorize_request
        pub fn authorize_request(c: &Case, action: Action) -> Result<(), Denial> {
            manage_action(c, action)?;
            authorize_job(c)
        }
    }

    fn facts(admin: bool, grants: Grants, disabled: bool) -> Facts {
        Facts {
            user: Some(User {
                enabled: true,
                admin,
            }),
            dev_scope: true,
            delegation: Delegation::None,
            grants: ENVIRONMENTS
                .iter()
                .map(|env| (("app".to_owned(), (*env).to_owned()), grants))
                .collect(),
            disabled: if disabled {
                HashSet::from(["app".to_owned()])
            } else {
                HashSet::new()
            },
        }
    }
    const USE: Grants = Grants {
        use_: true,
        develop: false,
        deploy: false,
    };
    const DEVELOP: Grants = Grants {
        use_: false,
        develop: true,
        deploy: false,
    };
    const NONE: Grants = Grants {
        use_: false,
        develop: false,
        deploy: false,
    };

    #[test]
    fn use_and_admin_reads() {
        assert!(
            facts(false, USE, false)
                .decide("app", "prod", AppAction::Use)
                .is_ok()
        );
        assert!(
            facts(false, NONE, false)
                .decide("app", "prod", AppAction::Use)
                .is_err()
        );
        assert!(
            facts(true, NONE, false)
                .decide("app", "prod", AppAction::Use)
                .is_ok()
        );
    }

    #[test]
    fn company_switch_blocks_use_for_everyone_but_developers_in_dev() {
        let disabled = Err(Denial::AppDisabled);
        assert_eq!(
            facts(true, NONE, true).decide("app", "prod", AppAction::Use),
            disabled
        );
        assert_eq!(
            facts(false, DEVELOP, true).decide("app", "prod", AppAction::Use),
            disabled
        );
        assert_eq!(
            facts(false, USE, true).decide("app", "dev", AppAction::Use),
            disabled
        );
        assert!(
            facts(true, NONE, true)
                .decide("app", "dev", AppAction::Use)
                .is_ok()
        );
        assert!(
            facts(false, DEVELOP, true)
                .decide("app", "dev", AppAction::Use)
                .is_ok()
        );
        // Management is not use: the switch does not stop runtime control.
        assert!(
            facts(true, NONE, true)
                .decide("app", "prod", AppAction::Manage(Action::Stop))
                .is_ok()
        );
    }

    #[test]
    fn writes_need_develop_and_prod_writes_need_admin() {
        assert!(
            facts(false, DEVELOP, true)
                .decide("app", "dev", AppAction::Develop)
                .is_ok()
        );
        assert!(
            facts(false, USE, false)
                .decide("app", "dev", AppAction::Develop)
                .is_err()
        );
        assert!(
            facts(false, DEVELOP, false)
                .decide("app", "prod", AppAction::Develop)
                .is_err()
        );
        assert!(
            facts(true, NONE, false)
                .decide("app", "prod", AppAction::Develop)
                .is_ok()
        );
    }

    fn scope(env: &str, actions: &[Action]) -> delegations::Scope {
        delegations::Scope {
            id: Uuid::nil(),
            user_id: "u".into(),
            name: "d".into(),
            app_id: "app".into(),
            environment: env.into(),
            actions: actions.iter().map(|a| a.as_str().to_owned()).collect(),
            expires_at: Utc::now(),
        }
    }

    fn every_facts() -> Vec<Facts> {
        let users = [
            None,
            Some(User {
                enabled: false,
                admin: false,
            }),
            Some(User {
                enabled: false,
                admin: true,
            }),
            Some(User {
                enabled: true,
                admin: false,
            }),
            Some(User {
                enabled: true,
                admin: true,
            }),
        ];
        let mut delegations = vec![Delegation::None, Delegation::Inactive];
        for env in ENVIRONMENTS {
            for actions in [
                vec![],
                vec![Action::Read],
                vec![Action::Read, Action::Start],
                vec![Action::Promote],
                vec![Action::Read, Action::Promote, Action::AppRead],
            ] {
                delegations.push(Delegation::Active(scope(env, &actions)));
            }
        }
        let mut all = vec![];
        for user in users {
            for dev_scope in [false, true] {
                for delegation in &delegations {
                    for bits in 0..64u8 {
                        let mut grants = HashMap::new();
                        for (index, env) in ENVIRONMENTS.iter().enumerate() {
                            let b = bits >> (index * 3);
                            grants.insert(
                                ("app".to_owned(), (*env).to_owned()),
                                Grants {
                                    use_: b & 1 != 0,
                                    develop: b & 2 != 0,
                                    deploy: b & 4 != 0,
                                },
                            );
                        }
                        for disabled in [false, true] {
                            all.push(Facts {
                                user,
                                dev_scope,
                                delegation: delegation.clone(),
                                grants: grants.clone(),
                                disabled: if disabled {
                                    HashSet::from(["app".to_owned()])
                                } else {
                                    HashSet::new()
                                },
                            });
                        }
                    }
                }
            }
        }
        all
    }

    #[test]
    fn decisions_reproduce_the_replaced_checks_for_every_combination() {
        let actions = [
            Action::Read,
            Action::AppRead,
            Action::Start,
            Action::Promote,
            Action::Build,
        ];
        let mut compared = 0;
        for facts in every_facts() {
            for env in ENVIRONMENTS {
                let c = legacy::Case {
                    facts: &facts,
                    app: "app",
                    env,
                };
                let decide = |action| facts.decide("app", env, action);
                assert_eq!(decide(AppAction::Use), legacy::authorize(&c, false));
                assert_eq!(decide(AppAction::Develop), legacy::authorize(&c, true));
                assert_eq!(decide(AppAction::FollowJob), legacy::authorize_job(&c));
                assert_eq!(facts.manages("app", env), legacy::can_manage(&c));
                for action in actions {
                    assert_eq!(
                        decide(AppAction::Manage(action)),
                        legacy::manage_action(&c, action),
                        "{action:?} {env}"
                    );
                    assert_eq!(
                        decide(AppAction::Request(action)),
                        legacy::authorize_request(&c, action),
                        "{action:?} {env}"
                    );
                }
                compared += 1;
            }
        }
        assert!(compared > 30_000, "matrix shrank to {compared}");
    }

    #[test]
    fn flags_mirror_the_decisions_and_lists_use_the_same_pairs() {
        for facts in every_facts() {
            let apps = vec!["app".to_owned(), "other".to_owned()];
            for action in [AppAction::Use, AppAction::FollowJob] {
                let (listed_apps, listed_envs) = facts.allowed_pairs(&apps, action);
                for app in &apps {
                    for env in ENVIRONMENTS {
                        let listed = listed_apps
                            .iter()
                            .zip(&listed_envs)
                            .any(|(a, e)| a == app && e == env);
                        assert_eq!(listed, facts.decide(app, env, action).is_ok());
                    }
                }
            }
            for env in ENVIRONMENTS {
                let flags = facts.flags("app", env);
                assert_eq!(
                    flags.can_open,
                    facts.decide("app", env, AppAction::Use).is_ok()
                );
                assert_eq!(
                    flags.can_manage,
                    facts
                        .decide("app", env, AppAction::Manage(Action::Read))
                        .is_ok()
                );
                assert_eq!(
                    flags.can_promote,
                    facts
                        .decide("app", "prod", AppAction::Request(Action::Promote))
                        .is_ok()
                );
            }
        }
    }
}
