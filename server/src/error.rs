//! Errores de la API REST propia (`/api/v1`). Formato: `{"error": "codigo", "message": "texto"}`.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("no has iniciado sesión")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("no encontrado")]
    NotFound,
    #[error("{0}")]
    BadGateway(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            AppError::BadGateway(_) => (StatusCode::BAD_GATEWAY, "bad_gateway"),
            AppError::Db(_) | AppError::Other(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        let message = match &self {
            AppError::Db(e) => {
                tracing::error!(error = %e, "error de base de datos");
                "error interno".to_string()
            }
            AppError::Other(e) => {
                tracing::error!(error = %e, "error interno");
                "error interno".to_string()
            }
            e => e.to_string(),
        };
        (status, Json(json!({"error": code, "message": message}))).into_response()
    }
}
