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
    pub id: String,
    pub scopes: Vec<String>,
    pub csrf: Option<String>,
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
    client: reqwest::Client,
    discovery: Discovery,
    web_secret: String,
    resource_secret: String,
    jwks: JwkSet,
}

pub fn random() -> String {
    let mut b = [0; 32];
    OsRng.fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
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
pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|c| {
            let (k, v) = c.trim().split_once('=')?;
            (k == name).then(|| v.to_string())
        })
}
impl Auth {
    pub async fn new() -> anyhow::Result<Self> {
        let cert = reqwest::Certificate::from_pem(&std::fs::read(std::env::var("MIY_CA_FILE")?)?)?;
        let client = reqwest::Client::builder()
            .add_root_certificate(cert)
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let issuer = std::env::var("MIY_ISSUER")?;
        let discovery: Discovery = client
            .get(format!("{issuer}/.well-known/openid-configuration"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        anyhow::ensure!(discovery.issuer == issuer, "issuer_mismatch");
        let jwks = client
            .get(&discovery.jwks_uri)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(Self {
            origin: std::env::var("MIY_ORIGIN")?,
            issuer,
            client,
            discovery,
            jwks,
            web_secret: std::env::var("MIY_WEB_SECRET")?,
            resource_secret: std::env::var("MIY_RESOURCE_SECRET")?,
        })
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
            id: c.sub,
            scopes,
            csrf: None,
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
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id=$1)")
            .bind(&id.sub)
            .fetch_one(&state.pool)
            .await?;
        if !exists {
            return Err(ApiError::forbidden());
        }
        let session = random();
        sqlx::query("INSERT INTO sessions(id_hash,user_id,access_token,refresh_token,csrf,expires_at) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(hash(&session)).bind(id.sub).bind(encrypt(&tokens.access_token)?).bind(encrypt(tokens.refresh_token.as_deref().unwrap_or(""))?)
            .bind(random()).bind(Utc::now()+Duration::seconds(tokens.expires_in)).execute(&state.pool).await?;
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
        let session = cookie(headers, "miy_session").ok_or_else(ApiError::unauthorized)?;
        let mut tx = state.pool.begin().await?;
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
            id,
            scopes: vec!["miy.read".into(), "miy.dev".into()],
            csrf: Some(row.get("csrf")),
        })
    }
    pub async fn logout(&self, state: &State, headers: &HeaderMap) -> ApiResult<String> {
        if let Some(session) = cookie(headers, "miy_session") {
            sqlx::query("DELETE FROM sessions WHERE id_hash=$1")
                .bind(hash(session))
                .execute(&state.pool)
                .await?;
        }
        let mut url = Url::parse(&self.discovery.end_session_endpoint)?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", "miy-web"),
            ("post_logout_redirect_uri", &format!("{}/", self.origin)),
        ]);
        Ok(url.into())
    }
}
use sha2::Digest;
