//! Wiring: state, routes, middleware. Kept separate from `main.rs` so the
//! integration tests can build the same router against a temporary database.

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod notes;
pub mod rooms;
pub mod ws;
pub mod ydoc;

use axum::{
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderValue, Method,
    },
    middleware,
    routing::{get, post},
    Router,
};
use config::Config;
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub rooms: Arc<rooms::Rooms>,
    pub tickets: Arc<ws::Tickets>,
}

pub fn router(state: AppState) -> Router {
    // Everything here needs a live bearer token. `route_layer` (not `layer`)
    // matters: it only runs the check for paths that actually matched, so an
    // unknown URL still 404s instead of 401-ing.
    let protected = Router::new()
        .route("/notes", get(notes::list).post(notes::create))
        .route("/notes/{id}", get(notes::get_one).delete(notes::delete))
        .route("/sync", get(notes::sync))
        .route("/ws-ticket", post(ws::issue_ticket))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_token,
        ));

    let api = Router::new()
        .route("/auth/status", get(auth::status))
        .route("/auth/setup", post(auth::setup))
        .route("/auth/login", post(auth::login))
        // Outside the bearer middleware on purpose: a browser cannot set headers
        // on a WebSocket handshake, so this route authenticates with the
        // single-use ticket that `/ws-ticket` hands out.
        .route("/ws", get(ws::upgrade))
        .merge(protected);

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest(notedock_api::API_PREFIX, api);

    // In production the server also hands out the web client, which means the
    // browser talks to a single origin and CORS never enters the picture.
    if let Some(dir) = &state.config.web_dir {
        let index = dir.join("index.html");
        app = app.fallback_service(ServeDir::new(dir).not_found_service(ServeFile::new(index)));
    }

    app.layer(cors_layer(&state.config.cors_origins))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn cors_layer(origins: &[String]) -> CorsLayer {
    let allowed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| match origin.parse::<HeaderValue>() {
            Ok(value) => Some(value),
            Err(_) => {
                tracing::warn!(%origin, "ignoring unparseable CORS origin");
                None
            }
        })
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE])
}

/// Builds state and router from a config, creating the database if needed.
pub async fn build(config: Config) -> anyhow::Result<Router> {
    let pool = db::connect(&config.db_path).await?;
    let rooms = Arc::new(rooms::Rooms::new(pool.clone()));
    rooms::spawn_flusher(Arc::clone(&rooms));

    Ok(router(AppState {
        pool,
        config: Arc::new(config),
        rooms,
        tickets: Arc::new(ws::Tickets::default()),
    }))
}
