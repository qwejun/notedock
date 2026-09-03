//! Password login and bearer-token checking.
//!
//! Only a SHA-256 of each issued token is stored, so the database alone cannot
//! be replayed against the API. The password itself is never stored, only its
//! Argon2 hash.

use crate::{
    db::now_rfc3339,
    error::{AppError, AppResult},
    AppState,
};
use anyhow::anyhow;
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};
use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
    Json,
};
use notedock_api::{AuthStatusResponse, LoginRequest, LoginResponse, SetupRequest};
use sha2::{Digest, Sha256};
use std::time::Duration;
use uuid::Uuid;

/// Delay applied to failed logins. Argon2 verification already costs ~50ms, so
/// this is belt-and-braces; a lockout is deliberately avoided because on a
/// single-user server it would let anyone lock the owner out.
const FAILED_LOGIN_DELAY: Duration = Duration::from_millis(300);

pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    // A v4 UUID is 16 bytes straight from the OS CSPRNG, which is exactly what
    // a salt needs. Going through `uuid` rather than `OsRng` avoids depending on
    // argon2's optional rand_core/getrandom feature chain.
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
        .map_err(|err| anyhow!("failed to encode password salt: {err}"))?;
    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| anyhow!("failed to hash password: {err}"))
}

fn verify_password(plain: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(err) => {
            tracing::error!(%err, "configured password hash is not a valid PHC string");
            false
        }
    }
}

/// 64 hex chars drawn from the OS CSPRNG by way of two v4 UUIDs — ~244 bits.
/// Using `uuid` here keeps the dependency list one crate shorter than pulling
/// in `rand` just for this.
fn new_token() -> String {
    format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

pub fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

async fn stored_password_hash(state: &AppState) -> AppResult<Option<String>> {
    if !state.config.password_hash.trim().is_empty() {
        return Ok(Some(state.config.password_hash.clone()));
    }

    sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'password_hash'")
        .fetch_optional(&state.pool)
        .await
        .map_err(AppError::from)
}

async fn create_session(state: &AppState, label: Option<&str>) -> AppResult<LoginResponse> {
    let now = chrono::Utc::now();
    let expires_at = (now + chrono::Duration::days(state.config.session_ttl_days))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let token = new_token();

    sqlx::query("DELETE FROM sessions WHERE expires_at <= ?1")
        .bind(now_rfc3339())
        .execute(&state.pool)
        .await?;
    sqlx::query(
        "INSERT INTO sessions (token_hash, label, created_at, expires_at) \
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(token_hash(&token))
    .bind(label)
    .bind(now_rfc3339())
    .bind(&expires_at)
    .execute(&state.pool)
    .await?;

    Ok(LoginResponse { token, expires_at })
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<AuthStatusResponse>> {
    Ok(Json(AuthStatusResponse {
        initialized: stored_password_hash(&state).await?.is_some(),
    }))
}

pub async fn setup(
    State(state): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> AppResult<Json<LoginResponse>> {
    if req.password.chars().count() < 8 {
        return Err(AppError::BadRequest("密码至少需要 8 个字符".to_owned()));
    }
    if stored_password_hash(&state).await?.is_some() {
        return Err(AppError::BadRequest("密码已经设置，请直接登录".to_owned()));
    }

    let hash = hash_password(&req.password).map_err(AppError::Internal)?;
    let result = sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES ('password_hash', ?1) \
         ON CONFLICT(key) DO NOTHING",
    )
    .bind(hash)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("密码已经设置，请直接登录".to_owned()));
    }

    Ok(Json(create_session(&state, req.label.as_deref()).await?))
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<LoginResponse>> {
    let Some(password_hash) = stored_password_hash(&state).await? else {
        return Err(AppError::BadRequest("尚未设置密码，请先打开 Web 端完成初始化".to_owned()));
    };
    if !verify_password(&req.password, &password_hash) {
        tokio::time::sleep(FAILED_LOGIN_DELAY).await;
        return Err(AppError::Unauthorized);
    }

    Ok(Json(create_session(&state, req.label.as_deref()).await?))
}

/// Rejects anything without a live bearer token. Applied to the whole `/api/v1`
/// tree except `/auth/login`.
pub async fn require_token(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(AppError::Unauthorized)?;

    let live: Option<(String,)> = sqlx::query_as(
        "SELECT token_hash FROM sessions WHERE token_hash = ?1 AND expires_at > ?2",
    )
    .bind(token_hash(token))
    .bind(now_rfc3339())
    .fetch_optional(&state.pool)
    .await?;

    if live.is_none() {
        return Err(AppError::Unauthorized);
    }

    Ok(next.run(req).await)
}
