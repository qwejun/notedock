//! Runtime configuration, all from the environment so the Docker image needs no
//! config file baked in.

use anyhow::{bail, Context};
use std::{env, net::SocketAddr, path::PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub db_path: PathBuf,
    /// Argon2 PHC string. There is deliberately no default: a NoteDock reachable
    /// from the public internet must not ship with a known password.
    pub password_hash: String,
    pub session_ttl_days: i64,
    /// Origins allowed to call the API from a browser. Only needed while the web
    /// client runs on the Vite dev server; in production the server serves it.
    pub cors_origins: Vec<String>,
    /// Directory of built web-client assets to serve at `/`. Unset in dev.
    pub web_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind = env::var("NOTEDOCK_BIND")
            .unwrap_or_else(|_| "[::]:8080".to_owned())
            .parse()
            .context("NOTEDOCK_BIND must look like `[::]:8080`")?;

        let db_path = env::var("NOTEDOCK_DB")
            .unwrap_or_else(|_| "data/notedock.db".to_owned())
            .into();

        let password_hash = match (
            env::var("NOTEDOCK_PASSWORD_HASH"),
            env::var("NOTEDOCK_PASSWORD"),
        ) {
            (Ok(hash), _) if !hash.trim().is_empty() => hash,
            // Convenience for local development only: hash the plaintext at
            // boot. Logged loudly because the password then lives in the
            // process environment, where `ps` and crash dumps can see it.
            (_, Ok(plain)) if !plain.trim().is_empty() => {
                tracing::warn!(
                    "using NOTEDOCK_PASSWORD; prefer NOTEDOCK_PASSWORD_HASH \
                     (generate one with `notedock-server hash-password <pw>`)"
                );
                crate::auth::hash_password(&plain)?
            }
            _ => bail!(
                "set NOTEDOCK_PASSWORD_HASH (or NOTEDOCK_PASSWORD for local dev). \
                 Generate a hash with `notedock-server hash-password <password>`"
            ),
        };

        let session_ttl_days = env::var("NOTEDOCK_SESSION_TTL_DAYS")
            .ok()
            .map(|v| v.parse())
            .transpose()
            .context("NOTEDOCK_SESSION_TTL_DAYS must be an integer")?
            .unwrap_or(30);

        let cors_origins = env::var("NOTEDOCK_CORS_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:5173,http://127.0.0.1:5173".to_owned())
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();

        let web_dir = env::var("NOTEDOCK_WEB_DIR").ok().map(PathBuf::from);

        Ok(Self {
            bind,
            db_path,
            password_hash,
            session_ttl_days,
            cors_origins,
            web_dir,
        })
    }
}
