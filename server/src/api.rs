//! API REST propia (`/api/v1`).

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;

use crate::{
    auth::{self, CurrentUser},
    connections,
    error::AppResult,
    state::{AppState, NowPlaying},
    tokens,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/salud", get(salud))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/me", get(auth::me).patch(auth::update_me))
        .route("/api/v1/tokens", get(tokens::list).post(tokens::create))
        .route("/api/v1/tokens/{id}", delete(tokens::delete))
        .route("/api/v1/connections", get(connections::list))
        .route(
            "/api/v1/connections/{provider}",
            put(connections::connect).patch(connections::patch).delete(connections::delete),
        )
        .route("/api/v1/forward/status", get(connections::status))
        .route("/api/v1/forward/retry", post(connections::retry))
        .route("/api/v1/scrobbles", get(scrobbles))
        .route("/api/v1/now-playing", get(now_playing))
}

/// GET /api/v1/salud
async fn salud(State(st): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").execute(&st.db).await.is_ok();
    Json(json!({"status": if db_ok { "ok" } else { "error" }, "version": env!("CARGO_PKG_VERSION"), "db": db_ok}))
}

#[derive(Deserialize)]
pub struct ScrobbleQuery {
    pub limit: Option<i64>,
    /// Devuelve las escuchas anteriores a este instante (paginación).
    pub before: Option<i64>,
}

#[derive(Serialize, FromRow)]
pub struct ScrobbleItem {
    pub id: i64,
    pub played_at: i64,
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub source: String,
}

/// GET /api/v1/scrobbles — historial del usuario, de lo más nuevo a lo más viejo.
async fn scrobbles(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Query<ScrobbleQuery>,
) -> AppResult<Json<Value>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let before = q.before.unwrap_or(i64::MAX);
    let items: Vec<ScrobbleItem> = sqlx::query_as(
        "SELECT s.id, s.played_at, a.name AS artist, t.title AS track, al.title AS album, \
                COALESCE(s.duration_ms, t.duration_ms) AS duration_ms, s.source \
         FROM scrobbles s JOIN artists a ON a.id = s.artist_id JOIN tracks t ON t.id = s.track_id \
         LEFT JOIN albums al ON al.id = s.album_id \
         WHERE s.user_id = ? AND s.played_at < ? ORDER BY s.played_at DESC, s.id DESC LIMIT ?",
    )
    .bind(u.id)
    .bind(before)
    .bind(limit)
    .fetch_all(&st.db)
    .await?;
    let (total,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM scrobbles WHERE user_id = ?").bind(u.id).fetch_one(&st.db).await?;
    let next = if items.len() as i64 == limit { items.last().map(|i| i.played_at) } else { None };
    Ok(Json(json!({"total": total, "items": items, "next_before": next})))
}

/// GET /api/v1/now-playing — lo que está sonando (se olvida a los 15 min o al terminar).
async fn now_playing(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> Json<Option<NowPlaying>> {
    let np = st.now_playing.read().await.get(&u.id).cloned();
    let fresh = np.filter(|n| {
        let limit = n.duration_ms.map(|d| d / 1000 + 60).unwrap_or(900).min(3600);
        crate::db::now() - n.since <= limit
    });
    Json(fresh)
}
