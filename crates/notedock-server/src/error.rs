//! One error type for every handler, one JSON shape on the wire.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use notedock_api::{ApiErrorBody, ErrorCode};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),

    #[error("missing or invalid credentials")]
    Unauthorized,

    #[error("no such note")]
    NotFound,


    /// Anything unexpected. The cause is logged; the client only learns that it
    /// was our fault, never why.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(err.into())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::Internal(err.into())
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, ErrorCode::BadRequest, msg),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                ErrorCode::Unauthorized,
                "missing or invalid credentials".to_owned(),
            ),
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                ErrorCode::NotFound,
                "no such note".to_owned(),
            ),
            AppError::Internal(err) => {
                tracing::error!(error = ?err, "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorCode::Internal,
                    "internal error".to_owned(),
                )
            }
        };

        (status, Json(ApiErrorBody { code, message })).into_response()
    }
}
