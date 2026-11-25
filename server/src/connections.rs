//! Cuentas conectadas (ListenBrainz ahora; Last.fm en la fase 2) y estado del reenvío.

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;

use crate::{
    auth::CurrentUser,
    db::now,
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Serialize, FromRow)]
pub struct Connection {
    pub provider: String,
    pub remote_username: Option<String>,
    pub forward_enabled: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub updated_at: i64,
}

/// GET /api/v1/connections
pub async fn list(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Vec<Connection>>> {
    let rows = sqlx::query_as(
        "SELECT provider, remote_username, forward_enabled, status, last_error, updated_at FROM connections WHERE user_id = ? ORDER BY provider",
    )
    .bind(u.id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct LbConnect {
    pub token: String,
}

/// PUT /api/v1/connections/:provider — conecta una cuenta (de momento, ListenBrainz).
pub async fn connect(
    State(st): State<AppState>,
    user: CurrentUser,
    Path(provider): Path<String>,
    Json(req): Json<LbConnect>,
) -> AppResult<Json<Connection>> {
    match provider.as_str() {
        "listenbrainz" => connect_listenbrainz(st, user, req).await,
        _ => Err(AppError::NotFound),
    }
}

/// Comprueba el token con ListenBrainz y lo guarda cifrado (más su HMAC para reconocerlo).
async fn connect_listenbrainz(
    st: AppState,
    CurrentUser(u): CurrentUser,
    req: LbConnect,
) -> AppResult<Json<Connection>> {
    let token = req.token.trim();
    if token.is_empty() {
        return Err(AppError::BadRequest("pega tu token de ListenBrainz".into()));
    }
    let remote = match st.lb.validate_token(token).await {
        Ok(Some(name)) => name,
        Ok(None) => return Err(AppError::BadRequest("ListenBrainz dice que ese token no es válido".into())),
        Err(e) => return Err(AppError::BadGateway(e)),
    };
    let hmac = st.crypto.lookup_hmac(token);
    let other: Option<(i64,)> =
        sqlx::query_as("SELECT user_id FROM connections WHERE lookup_hmac = ? AND user_id <> ?")
            .bind(&hmac)
            .bind(u.id)
            .fetch_optional(&st.db)
            .await?;
    if other.is_some() {
        return Err(AppError::BadRequest("ese token de ListenBrainz ya lo tiene conectado otro usuario".into()));
    }
    let t = now();
    let mut tx = st.db.begin().await?;
    sqlx::query(
        "INSERT INTO connections (user_id, provider, remote_username, secret_enc, lookup_hmac, forward_enabled, status, last_error, updated_at) \
         VALUES (?, 'listenbrainz', ?, ?, ?, 1, 'ok', NULL, ?) \
         ON CONFLICT(user_id, provider) DO UPDATE SET remote_username = excluded.remote_username, secret_enc = excluded.secret_enc, \
           lookup_hmac = excluded.lookup_hmac, status = 'ok', last_error = NULL, updated_at = excluded.updated_at",
    )
    .bind(u.id)
    .bind(&remote)
    .bind(st.crypto.encrypt_str(token))
    .bind(&hmac)
    .bind(t)
    .execute(&mut *tx)
    .await?;
    resume(&mut tx, u.id, "listenbrainz").await?;
    tx.commit().await?;
    st.forward_wake.notify_one();
    get_one(&st, u.id, "listenbrainz").await.map(Json)
}

#[derive(Deserialize)]
pub struct Patch {
    pub forward_enabled: bool,
}

/// PATCH /api/v1/connections/:provider — activa o desactiva el reenvío.
pub async fn patch(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(provider): Path<String>,
    Json(req): Json<Patch>,
) -> AppResult<Json<Connection>> {
    let t = now();
    let mut tx = st.db.begin().await?;
    let r =
        sqlx::query("UPDATE connections SET forward_enabled = ?, updated_at = ? WHERE user_id = ? AND provider = ?")
            .bind(req.forward_enabled)
            .bind(t)
            .bind(u.id)
            .bind(&provider)
            .execute(&mut *tx)
            .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if req.forward_enabled {
        resume(&mut tx, u.id, &provider).await?;
    } else {
        // Lo pendiente se queda en espera (no se borra).
        sqlx::query("UPDATE forward_queue SET status = 'paused', updated_at = ? WHERE user_id = ? AND target = ? AND status = 'pending'")
            .bind(t)
            .bind(u.id)
            .bind(&provider)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    st.forward_wake.notify_one();
    get_one(&st, u.id, &provider).await.map(Json)
}

/// DELETE /api/v1/connections/:provider — borra la conexión y lo que quedaba por reenviar a ese destino.
pub async fn delete(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(provider): Path<String>,
) -> AppResult<Json<Value>> {
    let mut tx = st.db.begin().await?;
    let r = sqlx::query("DELETE FROM connections WHERE user_id = ? AND provider = ?")
        .bind(u.id)
        .bind(&provider)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM forward_queue WHERE user_id = ? AND target = ? AND status <> 'done'")
        .bind(u.id)
        .bind(&provider)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok": true})))
}

/// Reanuda lo que estaba en pausa o fallido para ese destino.
async fn resume(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, user_id: i64, target: &str) -> sqlx::Result<()> {
    let t = now();
    sqlx::query(
        "UPDATE forward_queue SET status = 'pending', next_attempt_at = ?, updated_at = ? \
         WHERE user_id = ? AND target = ? AND status IN ('paused', 'failed')",
    )
    .bind(t)
    .bind(t)
    .bind(user_id)
    .bind(target)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn get_one(st: &AppState, user_id: i64, provider: &str) -> AppResult<Connection> {
    sqlx::query_as(
        "SELECT provider, remote_username, forward_enabled, status, last_error, updated_at FROM connections WHERE user_id = ? AND provider = ?",
    )
    .bind(user_id)
    .bind(provider)
    .fetch_optional(&st.db)
    .await?
    .ok_or(AppError::NotFound)
}

#[derive(Serialize, FromRow)]
pub struct QueueCount {
    pub target: String,
    pub status: String,
    pub count: i64,
    pub last_error: Option<String>,
}

/// GET /api/v1/forward/status — cuántas escuchas hay en cada estado por destino.
pub async fn status(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Vec<QueueCount>>> {
    let rows = sqlx::query_as(
        "SELECT target, status, COUNT(*) AS count, MAX(last_error) AS last_error FROM forward_queue \
         WHERE user_id = ? GROUP BY target, status ORDER BY target, status",
    )
    .bind(u.id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(rows))
}

/// POST /api/v1/forward/retry — vuelve a intentar lo fallido o en pausa (si la conexión está bien).
pub async fn retry(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Value>> {
    let t = now();
    let r = sqlx::query(
        "UPDATE forward_queue SET status = 'pending', next_attempt_at = ?, updated_at = ? \
         WHERE user_id = ? AND status IN ('paused', 'failed') AND target IN \
           (SELECT provider FROM connections WHERE user_id = ? AND forward_enabled = 1 AND status = 'ok')",
    )
    .bind(t)
    .bind(t)
    .bind(u.id)
    .bind(u.id)
    .execute(&st.db)
    .await?;
    st.forward_wake.notify_one();
    Ok(Json(json!({"requeued": r.rows_affected()})))
}
