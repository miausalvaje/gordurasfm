//! API REST propia (`/api/v1`).

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    routing::{delete, get, patch, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;

use crate::{
    admin,
    auth::{self, CurrentUser},
    connections,
    error::AppResult,
    export, jobs, lastfm, media,
    state::{AppState, NowPlaying},
    stats, tokens, wrapped,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/salud", get(salud))
        .route("/api/v1/openapi.json", get(openapi))
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
        .route("/api/v1/connections/lastfm/start", get(lastfm::start))
        .route("/api/v1/connections/lastfm/callback", get(lastfm::callback))
        .route("/api/v1/connections/lastfm/key", put(lastfm::put_own_key).delete(lastfm::delete_own_key))
        .route("/api/v1/lastfm/status", get(lastfm::status))
        .route("/api/v1/scrobbles", get(scrobbles))
        .route("/api/v1/now-playing", get(now_playing))
        .route("/api/v1/stats", get(stats::get_stats))
        .route("/api/v1/top/{kind}", get(stats::get_top))
        .route("/api/v1/media/cover/{id}", get(media::cover))
        .route("/api/v1/media/artist/{id}", get(media::artist))
        .route("/api/v1/media/stream/{song_id}", get(media::stream))
        .route("/api/v1/wrapped", get(wrapped::list))
        .route("/api/v1/wrapped/image/{hash}", get(wrapped::image))
        .route("/api/v1/wrapped/{year}", get(wrapped::get))
        .route("/api/v1/wrapped/{year}/regenerate", post(wrapped::regenerate))
        .route("/api/v1/wrapped/answers/{id}", post(wrapped::answer))
        .route("/api/v1/import/lastfm", post(jobs::start_lastfm))
        .route(
            "/api/v1/import/lastfm/sync",
            get(lastfm::sync::get_sync).put(lastfm::sync::put_sync).delete(lastfm::sync::delete_sync),
        )
        .route("/api/v1/import/file", post(jobs::upload_file).layer(DefaultBodyLimit::max(jobs::MAX_UPLOAD)))
        .route("/api/v1/import/jobs", get(jobs::list))
        .route("/api/v1/import/jobs/{id}", get(jobs::get))
        .route("/api/v1/import/jobs/{id}/cancel", post(jobs::cancel))
        .route("/api/v1/import/jobs/{id}/resume", post(jobs::resume))
        .route("/api/v1/export", get(export::export))
        .route("/api/v1/admin/lastfm", get(admin::get_lastfm).put(admin::put_lastfm).delete(admin::delete_lastfm))
        .route("/api/v1/admin/users", get(admin::users))
        .route("/api/v1/admin/users/{id}", patch(admin::patch_user))
}

/// GET /api/v1/salud
async fn salud(State(st): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").execute(&st.db).await.is_ok();
    Json(json!({"status": if db_ok { "ok" } else { "error" }, "version": env!("CARGO_PKG_VERSION"), "db": db_ok}))
}

/// Especificación OpenAPI 3.1 (se edita en `docs/openapi.json`).
pub const OPENAPI: &str = include_str!("../../docs/openapi.json");

/// GET /api/v1/openapi.json
async fn openapi() -> impl axum::response::IntoResponse {
    ([(axum::http::header::CONTENT_TYPE, "application/json")], OPENAPI)
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
    #[serde(skip)]
    pub album_id: Option<i64>,
    #[serde(skip)]
    pub cover_ref: Option<String>,
    #[sqlx(skip)]
    pub cover: Option<String>,
}

/// GET /api/v1/scrobbles — historial del usuario, de lo más nuevo a lo más viejo.
async fn scrobbles(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Query<ScrobbleQuery>,
) -> AppResult<Json<Value>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let before = q.before.unwrap_or(i64::MAX);
    let mut items: Vec<ScrobbleItem> = sqlx::query_as(
        "SELECT s.id, s.played_at, a.name AS artist, t.title AS track, al.title AS album, \
                COALESCE(s.duration_ms, t.duration_ms) AS duration_ms, s.source, s.album_id, al.cover_ref \
         FROM scrobbles s JOIN artists a ON a.id = s.artist_id JOIN tracks t ON t.id = s.track_id \
         LEFT JOIN albums al ON al.id = s.album_id \
         WHERE s.user_id = ? AND s.played_at < ? ORDER BY s.played_at DESC, s.id DESC LIMIT ?",
    )
    .bind(u.id)
    .bind(before)
    .bind(limit)
    .fetch_all(&st.db)
    .await?;
    for it in &mut items {
        if let (Some(id), Some(_)) = (it.album_id, &it.cover_ref) {
            it.cover = Some(format!("/api/v1/media/cover/{id}"));
        }
    }
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    /// Saca `(método, ruta)` de las llamadas `.route("…", get(…).post(…))` de un archivo fuente.
    fn routes_in(src: &str) -> BTreeSet<(String, String)> {
        let mut out = BTreeSet::new();
        // Solo el código, no los tests (que también contienen `.route(` de ejemplo).
        let mut rest = src.split("#[cfg(test)]").next().unwrap();
        while let Some(i) = rest.find(".route(") {
            rest = rest[i + 7..].trim_start();
            let Some(r) = rest.strip_prefix('"') else { continue };
            rest = r;
            let end = rest.find('"').unwrap();
            let path = rest[..end].replace("{*rest}", "{rest}");
            // Hasta el paréntesis que cierra `.route(`.
            let (mut depth, mut j) = (1, 0);
            for (k, c) in rest.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    j = k;
                    break;
                }
            }
            let call = &rest[end..j];
            for m in ["get", "post", "put", "patch", "delete"] {
                let pat = format!("{m}(");
                if call.match_indices(&pat).any(|(k, _)| {
                    k == 0 || !call.as_bytes()[k - 1].is_ascii_alphanumeric() && call.as_bytes()[k - 1] != b'_'
                }) {
                    out.insert((m.to_string(), path.clone()));
                }
            }
            rest = &rest[j..];
        }
        out
    }

    #[test]
    fn openapi_cubre_todas_las_rutas() {
        let spec: serde_json::Value = serde_json::from_str(super::OPENAPI).expect("openapi.json no es JSON válido");
        assert_eq!(spec["openapi"], "3.1.0");
        assert_eq!(spec["info"]["version"], env!("CARGO_PKG_VERSION"), "versión de openapi.json");
        let mut documented = BTreeSet::new();
        for (path, ops) in spec["paths"].as_object().unwrap() {
            for (m, op) in ops.as_object().unwrap() {
                assert!(op["summary"].is_string(), "{m} {path} sin summary");
                documented.insert((m.clone(), path.clone()));
            }
        }
        let mut code = routes_in(include_str!("api.rs"));
        code.extend(routes_in(include_str!("listenbrainz/compat.rs")));
        assert!(code.len() > 40, "no se han encontrado las rutas: {code:?}");
        let missing: Vec<_> = code.difference(&documented).collect();
        let extra: Vec<_> = documented.difference(&code).collect();
        assert!(missing.is_empty(), "rutas sin documentar en docs/openapi.json: {missing:?}");
        assert!(extra.is_empty(), "rutas documentadas que no existen: {extra:?}");
        // Todas las referencias apuntan a esquemas que existen.
        let txt = super::OPENAPI;
        for (i, _) in txt.match_indices("#/components/schemas/") {
            let name: String = txt[i + 21..].chars().take_while(|c| c.is_alphanumeric()).collect();
            assert!(spec["components"]["schemas"][&name].is_object(), "falta el esquema {name}");
        }
    }
}
