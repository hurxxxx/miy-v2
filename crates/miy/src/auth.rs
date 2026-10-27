pub use crate::identity::{cookie, development_profile, random};
use crate::{
    State,
    contract::hash,
    http::{ApiError, ApiResult},
};
use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, OsRng, rand_core::RngCore},
};
use axum::http::HeaderMap;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use serde::Deserialize;
use sqlx::Row;
use url::Url;

#[derive(Clone, Debug)]
pub struct Principal {
    pub cli_session_id: Option<uuid::Uuid>,
    pub id: String,
    pub scopes: Vec<String>,
    pub csrf: Option<String>,
    pub development: bool,
}
#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    introspection_endpoint: String,
    jwks_uri: String,
    end_session_endpoint: String,
}
#[derive(Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_in: i64,
}
#[derive(Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    nonce: String,
}
#[derive(Deserialize)]
struct Introspection {
    active: bool,
    #[serde(default)]
    sub: String,
}
pub struct Auth {
    pub origin: String,
    pub issuer: String,
    pub development_origin: String,
    pub development_login: bool,
    client: reqwest::Client,
    discovery: Discovery,
    web_secret: String,
    resource_secret: String,
    jwks: JwkSet,
}

pub fn encrypt(value: &str) -> anyhow::Result<String> {
    let key = hex::decode(std::env::var("MIY_ENCRYPTION_KEY")?)?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| anyhow::anyhow!("invalid_encryption_key"))?;
    let mut nonce = [0; 12];
    OsRng.fill_bytes(&mut nonce);
    let bytes = cipher
        .encrypt((&nonce).into(), value.as_bytes())
        .map_err(|_| anyhow::anyhow!("encryption_failed"))?;
    Ok(URL_SAFE_NO_PAD.encode([nonce.as_slice(), bytes.as_slice()].concat()))
}
pub fn decrypt(value: &str) -> anyhow::Result<String> {
    let key = hex::decode(std::env::var("MIY_ENCRYPTION_KEY")?)?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| anyhow::anyhow!("invalid_encryption_key"))?;
    let bytes = URL_SAFE_NO_PAD.decode(value)?;
    anyhow::ensure!(bytes.len() >= 28, "invalid_ciphertext");
    let plain = cipher
        .decrypt(bytes[..12].into(), &bytes[12..])
        .map_err(|_| anyhow::anyhow!("decryption_failed"))?;
    Ok(String::from_utf8(plain)?)
}
impl Auth {
    pub async fn new() -> anyhow::Result<Self> {
        let allow_http = std::env::var("MIY_ALLOW_HTTP").as_deref() == Ok("1");
        let origin = crate::server_origin::validate(&std::env::var("MIY_ORIGIN")?, allow_http)?;
        let issuer = crate::server_origin::issuer(&std::env::var("MIY_ISSUER")?, allow_http)?;
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(10))
            .retry(reqwest::retry::never());
        if let Some(path) = std::env::var_os("MIY_CA_FILE").filter(|value| !value.is_empty()) {
            let cert = reqwest::Certificate::from_pem(&std::fs::read(path)?)?;
            builder = builder.add_root_certificate(cert);
        }
        let client = builder.build()?;
        let discovery: Discovery = client
            .get(format!("{issuer}/.well-known/openid-configuration"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        anyhow::ensure!(discovery.issuer == issuer, "issuer_mismatch");
        for endpoint in [
            &discovery.authorization_endpoint,
            &discovery.token_endpoint,
            &discovery.introspection_endpoint,
            &discovery.jwks_uri,
            &discovery.end_session_endpoint,
        ] {
            let endpoint = Url::parse(endpoint)?;
            anyhow::ensure!(
                endpoint.username().is_empty()
                    && endpoint.password().is_none()
                    && endpoint.fragment().is_none()
                    && (endpoint.scheme() == "https"
                        || (issuer.starts_with("http://") && endpoint.scheme() == "http")),
                "invalid_identity_endpoint_transport"
            );
        }
        let jwks = client
            .get(&discovery.jwks_uri)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let development_origin =
            std::env::var("MIY_DEV_ORIGIN").unwrap_or_else(|_| "http://localhost:18000".into());
        let development_login = development_profile(
            std::env::var("MIY_DEV_LOGIN").ok().as_deref(),
            &origin,
            &development_origin,
        );
        Ok(Self {
            origin,
            development_origin,
            development_login,
            issuer,
            client,
            discovery,
            jwks,
            web_secret: std::env::var("MIY_WEB_SECRET")?,
            resource_secret: std::env::var("MIY_RESOURCE_SECRET")?,
        })
    }
    pub fn cookie(&self, name: &str, value: &str, path: &str, max_age: u32) -> String {
        crate::server_origin::cookie(&self.origin, name, value, path, max_age)
    }
    fn claims(&self, token: &str, audience: &str) -> ApiResult<Claims> {
        let header = jsonwebtoken::decode_header(token).map_err(|_| ApiError::unauthorized())?;
        let jwk = self
            .jwks
            .find(header.kid.as_deref().unwrap_or(""))
            .ok_or_else(ApiError::unauthorized)?;
        let key = DecodingKey::from_jwk(jwk).map_err(|_| ApiError::unauthorized())?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[audience]);
        validation.set_issuer(&[&self.issuer]);
        validation.leeway = 0;
        jsonwebtoken::decode::<Claims>(token, &key, &validation)
            .map(|d| d.claims)
            .map_err(|_| ApiError::unauthorized())
    }
    async fn active(&self, token: &str, web: bool) -> ApiResult<String> {
        let resource_id = format!("{}/mcp", self.origin);
        let (client_id, secret) = if web {
            ("miy-web", &self.web_secret)
        } else {
            (resource_id.as_str(), &self.resource_secret)
        };
        let r = self
            .client
            .post(&self.discovery.introspection_endpoint)
            .form(&[
                ("client_id", client_id),
                ("client_secret", secret),
                ("token", token),
            ])
            .send()
            .await
            .map_err(|_| ApiError::unavailable("identity_unavailable"))?;
        if !r.status().is_success() {
            return Err(ApiError::unavailable("identity_unavailable"));
        }
        let data: Introspection = r.json().await.map_err(|_| ApiError::unauthorized())?;
        if !data.active || data.sub.is_empty() {
            tracing::warn!(
                event = "introspection_rejected",
                active = data.active,
                subject_present = !data.sub.is_empty()
            );
        }
        if !data.active {
            return Err(ApiError::unauthorized());
        }
        Ok(data.sub)
    }
    pub async fn bearer(&self, token: &str) -> ApiResult<Principal> {
        let c = self.claims(token, &format!("{}/mcp", self.origin))?;
        if self.active(token, false).await? != c.sub {
            return Err(ApiError::unauthorized());
        }
        let scopes: Vec<_> = c.scope.split_whitespace().map(str::to_string).collect();
        if !scopes.iter().any(|s| s == "miy.read") {
            return Err(ApiError::forbidden());
        }
        Ok(Principal {
            cli_session_id: None,
            id: c.sub,
            scopes,
            csrf: None,
            development: false,
        })
    }
    pub async fn login(&self, state: &State) -> ApiResult<(String, String)> {
        let value = random();
        let verifier = random();
        let nonce = random();
        sqlx::query("DELETE FROM login_flows WHERE expires_at < now()")
            .execute(&state.pool)
            .await?;
        sqlx::query("INSERT INTO login_flows VALUES($1,$2,$3,$4)")
            .bind(hash(&value))
            .bind(&verifier)
            .bind(&nonce)
            .bind(Utc::now() + Duration::minutes(5))
            .execute(&state.pool)
            .await?;
        let mut url = Url::parse(&self.discovery.authorization_endpoint)?;
        let challenge = URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()));
        url.query_pairs_mut().extend_pairs([
            ("client_id", "miy-web"),
            ("response_type", "code"),
            ("scope", "openid miy.read miy.dev"),
            ("redirect_uri", &format!("{}/auth/callback", self.origin)),
            ("state", &value),
            ("nonce", &nonce),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ]);
        Ok((url.into(), value))
    }
    pub async fn callback(
        &self,
        state: &State,
        code: &str,
        value: &str,
        headers: &HeaderMap,
    ) -> ApiResult<String> {
        if cookie(headers, "miy_login").as_deref() != Some(value) {
            return Err(ApiError::unauthorized());
        }
        let row=sqlx::query("DELETE FROM login_flows WHERE state_hash=$1 AND expires_at>now() RETURNING verifier,nonce")
            .bind(hash(value)).fetch_optional(&state.pool).await?.ok_or_else(ApiError::unauthorized)?;
        let verifier: String = row.get("verifier");
        let tokens = self
            .exchange(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", &format!("{}/auth/callback", self.origin)),
                ("code_verifier", &verifier),
            ])
            .await?;
        let id = self.claims(
            tokens
                .id_token
                .as_deref()
                .ok_or_else(ApiError::unauthorized)?,
            "miy-web",
        )?;
        if id.nonce != row.get::<String, _>("nonce") {
            return Err(ApiError::unauthorized());
        }
        let mut tx = state.pool.begin().await?;
        let enabled: Option<bool> =
            sqlx::query_scalar("SELECT enabled FROM users WHERE id=$1 FOR SHARE")
                .bind(&id.sub)
                .fetch_optional(&mut *tx)
                .await?;
        if enabled != Some(true) {
            return Err(ApiError::forbidden());
        }
        let session = random();
        sqlx::query("INSERT INTO sessions(id_hash,user_id,access_token,refresh_token,csrf,expires_at) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(hash(&session)).bind(id.sub).bind(encrypt(&tokens.access_token)?).bind(encrypt(tokens.refresh_token.as_deref().unwrap_or(""))?)
            .bind(random()).bind(Utc::now()+Duration::seconds(tokens.expires_in)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(session)
    }
    async fn exchange(&self, values: &[(&str, &str)]) -> ApiResult<Tokens> {
        let mut form = values.to_vec();
        form.extend([
            ("client_id", "miy-web"),
            ("client_secret", self.web_secret.as_str()),
        ]);
        let r = self
            .client
            .post(&self.discovery.token_endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|_| ApiError::unavailable("identity_unavailable"))?;
        if !r.status().is_success() {
            return Err(ApiError::unauthorized());
        }
        r.json().await.map_err(|_| ApiError::unauthorized())
    }
    pub async fn session(&self, state: &State, headers: &HeaderMap) -> ApiResult<Principal> {
        if self.development_login
            && let Some(session) = cookie(headers, "miy_development")
        {
            let row = sqlx::query("SELECT user_id,csrf FROM development_sessions WHERE id_hash=$1 AND expires_at>now()")
                .bind(hash(session)).fetch_optional(&state.pool).await?.ok_or_else(ApiError::unauthorized)?;
            return Ok(Principal {
                cli_session_id: None,
                id: row.get("user_id"),
                scopes: vec!["miy.read".into(), "miy.dev".into()],
                csrf: Some(row.get("csrf")),
                development: true,
            });
        }
        let session = cookie(headers, "miy_session").ok_or_else(ApiError::unauthorized)?;
        let owner: String = sqlx::query_scalar("SELECT user_id FROM sessions WHERE id_hash=$1")
            .bind(hash(&session))
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(ApiError::unauthorized)?;
        let mut tx = state.pool.begin().await?;
        let enabled: Option<bool> =
            sqlx::query_scalar("SELECT enabled FROM users WHERE id=$1 FOR SHARE")
                .bind(&owner)
                .fetch_optional(&mut *tx)
                .await?;
        if enabled != Some(true) {
            return Err(ApiError::unauthorized());
        }
        let row=sqlx::query("SELECT * FROM sessions WHERE id_hash=$1 AND created_at>now()-interval '8 hours' FOR UPDATE")
            .bind(hash(&session)).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::unauthorized)?;
        let mut access = decrypt(row.get("access_token"))?;
        if row.get::<chrono::DateTime<Utc>, _>("expires_at") < Utc::now() + Duration::seconds(5) {
            let refresh = decrypt(row.get("refresh_token"))?;
            let t = self
                .exchange(&[("grant_type", "refresh_token"), ("refresh_token", &refresh)])
                .await?;
            sqlx::query("UPDATE sessions SET access_token=$2,refresh_token=$3,expires_at=$4 WHERE id_hash=$1")
                .bind(hash(&session)).bind(encrypt(&t.access_token)?).bind(encrypt(t.refresh_token.as_deref().unwrap_or(&refresh))?)
                .bind(Utc::now()+Duration::seconds(t.expires_in)).execute(&mut *tx).await?;
            access = t.access_token;
        }
        let id: String = row.get("user_id");
        if self.active(&access, true).await? != id {
            tracing::warn!(event = "session_subject_mismatch");
            return Err(ApiError::unauthorized());
        }
        tx.commit().await?;
        Ok(Principal {
            cli_session_id: None,
            id,
            scopes: vec!["miy.read".into(), "miy.dev".into()],
            csrf: Some(row.get("csrf")),
            development: false,
        })
    }
    pub async fn logout(&self, state: &State, headers: &HeaderMap) -> ApiResult<String> {
        let development = self.development_login && cookie(headers, "miy_development").is_some();
        if let Some(session) = cookie(headers, "miy_development") {
            sqlx::query("DELETE FROM development_sessions WHERE id_hash=$1")
                .bind(hash(session))
                .execute(&state.pool)
                .await?;
        }
        if let Some(session) = cookie(headers, "miy_session") {
            sqlx::query("DELETE FROM sessions WHERE id_hash=$1")
                .bind(hash(session))
                .execute(&state.pool)
                .await?;
        }
        if development {
            return Ok(format!("{}/", self.development_origin));
        }
        let mut url = Url::parse(&self.discovery.end_session_endpoint)?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", "miy-web"),
            ("post_logout_redirect_uri", &format!("{}/", self.origin)),
        ]);
        Ok(url.into())
    }
    pub fn allowed_origin(&self, headers: &HeaderMap, development: bool) -> bool {
        let origin = headers.get("origin").and_then(|v| v.to_str().ok());
        origin == Some(self.origin.as_str())
            || (development
                && self.development_login
                && origin == Some(self.development_origin.as_str()))
    }
    pub async fn development_session(&self, state: &State, account: &str) -> ApiResult<String> {
        if !self.development_login {
            return Err(ApiError::not_found());
        }
        if !["admin", "developer", "alice", "bob"].contains(&account) {
            return Err(ApiError::bad("unknown_development_account"));
        }
        sqlx::query("DELETE FROM development_sessions WHERE expires_at<now()")
            .execute(&state.pool)
            .await?;
        let mut tx = state.pool.begin().await?;
        let id: String =
            sqlx::query_scalar("SELECT id FROM users WHERE name=$1 AND enabled FOR SHARE")
                .bind(account)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(ApiError::unauthorized)?;
        let session = random();
        sqlx::query("INSERT INTO development_sessions VALUES($1,$2,$3,$4)")
            .bind(hash(&session))
            .bind(&id)
            .bind(random())
            .bind(Utc::now() + Duration::hours(8))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        tracing::info!(event = "development_login", account);
        Ok(session)
    }
}
use sha2::Digest;
