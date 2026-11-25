//! Tokens del usuario: `scrobble` (el que se pega en Navidrome) y `api` (integraciones).

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{
    auth::CurrentUser,
    crypto,
    db::now,
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Serialize, FromRow)]
pub struct TokenInfo {
    pub id: i64,
    pub kind: String,
    pub hint: String,
    pub label: Option<String>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Deserialize)]
pub struct NewToken {
    pub kind: String,
    pub label: Option<String>,
}

#[derive(Serialize)]
pub struct CreatedToken {
    pub id: i64,
    pub kind: String,
    /// Solo se enseña una vez.
    pub token: String,
}

/// GET /api/v1/tokens
pub async fn list(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Vec<TokenInfo>>> {
    let rows = sqlx::query_as(
        "SELECT id, kind, hint, label, created_at, last_used_at FROM api_tokens WHERE user_id = ? ORDER BY id",
    )
    .bind(u.id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(rows))
}

/// POST /api/v1/tokens
pub async fn create(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Json(req): Json<NewToken>,
) -> AppResult<Json<CreatedToken>> {
    if !matches!(req.kind.as_str(), "scrobble" | "api") {
        return Err(AppError::BadRequest("kind debe ser «scrobble» o «api»".into()));
    }
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM api_tokens WHERE user_id = ?").bind(u.id).fetch_one(&st.db).await?;
    if n >= 20 {
        return Err(AppError::BadRequest("como mucho 20 tokens por usuario".into()));
    }
    let token = crypto::random_token();
    let hint = token[token.len() - 4..].to_string();
    let label = req.label.map(|l| l.trim().chars().take(60).collect::<String>()).filter(|l| !l.is_empty());
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO api_tokens (user_id, kind, token_hash, hint, label, created_at) VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(u.id)
    .bind(&req.kind)
    .bind(crypto::token_hash(&token))
    .bind(&hint)
    .bind(label)
    .bind(now())
    .fetch_one(&st.db)
    .await?;
    Ok(Json(CreatedToken { id, kind: req.kind, token }))
}

/// DELETE /api/v1/tokens/:id
pub async fn delete(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let r =
        sqlx::query("DELETE FROM api_tokens WHERE id = ? AND user_id = ?").bind(id).bind(u.id).execute(&st.db).await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({"ok": true})))
}
