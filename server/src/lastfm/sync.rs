//! Sincronización periódica con Last.fm: trae lo nuevo de los usuarios que la han activado.
//!
//! Sirve para quien escucha con un reproductor que no pasa por Navidrome (MusicBee desde un SMB, por
//! ejemplo) pero scrobblea a Last.fm. No crea trabajos (`jobs`): corre en segundo plano y deja su
//! estado en la tabla `lastfm_sync`.

use std::time::Duration;

use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};

use super::client::LfmError;
use crate::{
    auth::CurrentUser,
    db::now,
    error::{AppError, AppResult},
    jobs::{self, ImportStats},
    lastfm,
    state::AppState,
};

const PAGE_SIZE: i64 = 200;
/// Margen hacia atrás en cada ronda. Algunos reproductores guardan los scrobbles sin conexión y los
/// envían más tarde con su hora original; con este margen también se recogen.
const OVERLAP_SECS: i64 = 24 * 3600;

#[derive(sqlx::FromRow)]
struct SyncRow {
    user_id: i64,
    username: String,
    synced_to: i64,
}

/// Tarea de fondo: cada `sync_interval_secs` sincroniza a todos los usuarios que lo tienen activado.
pub async fn run(st: AppState) {
    let every = st.cfg.lastfm.sync_interval_secs;
    if every == 0 {
        tracing::info!("sincronización con Last.fm desactivada (sync_interval_secs = 0)");
        return;
    }
    let every = Duration::from_secs(every);
    // La primera ronda no espera todo el intervalo.
    let mut wait = every.min(Duration::from_secs(20));
    loop {
        tokio::select! {
            _ = st.lastfm_sync_wake.notified() => {}
            _ = tokio::time::sleep(wait) => {}
        }
        wait = every;
        let rows: Vec<SyncRow> =
            match sqlx::query_as("SELECT user_id, username, synced_to FROM lastfm_sync").fetch_all(&st.db).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(error = %e, "sincronización con Last.fm");
                    continue;
                }
            };
        for row in rows {
            sync_user(&st, &row).await;
        }
    }
}

/// Una ronda para un usuario. Los fallos se guardan en `last_error` y se reintentan en la siguiente.
async fn sync_user(st: &AppState, row: &SyncRow) {
    // Si está importando a mano, que acabe primero (las dos cosas a la vez no rompen nada, pero no hace falta).
    if matches!(jobs::active_job(st, row.user_id).await, Ok(Some(_))) {
        return;
    }
    let started = now();
    let result = fetch_new(st, row).await;
    let (synced_to, error) = match &result {
        Ok(stats) => {
            if stats.stored > 0 {
                tracing::info!(usuario = row.user_id, nuevas = stats.stored, "sincronizado con Last.fm");
                st.enrich_wake.notify_one();
            }
            (started, None)
        }
        Err(e) => {
            tracing::warn!(usuario = row.user_id, error = %e, "sincronización con Last.fm fallida");
            (row.synced_to, Some(e.to_string()))
        }
    };
    if let Err(e) =
        sqlx::query("UPDATE lastfm_sync SET synced_to = ?, last_run_at = ?, last_error = ? WHERE user_id = ?")
            .bind(synced_to)
            .bind(started)
            .bind(error)
            .bind(row.user_id)
            .execute(&st.db)
            .await
    {
        tracing::error!(error = %e, "guardando el estado de la sincronización con Last.fm");
    }
}

async fn fetch_new(st: &AppState, row: &SyncRow) -> anyhow::Result<ImportStats> {
    let key = lastfm::key_for(st, row.user_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("falta configurar la API key de Last.fm"))?;
    let from = (row.synced_to - OVERLAP_SECS).max(0);
    let to = now() + 60;
    let delay = Duration::from_millis(st.cfg.lastfm.import_delay_ms);
    let mut stats = ImportStats::default();
    let mut page = 1;
    loop {
        let p = st
            .lastfm
            .recent_tracks(&key, &row.username, page, PAGE_SIZE, Some(from), Some(to))
            .await
            .map_err(|e: LfmError| anyhow::anyhow!(e.to_string()))?;
        let listens: Vec<_> = p.tracks.iter().filter_map(lastfm::listen_from_recent).collect();
        stats.invalid += (p.tracks.len() - listens.len()) as i64;
        jobs::ingest_all(st, row.user_id, &listens, &mut stats).await?;
        if p.tracks.is_empty() || page >= p.total_pages {
            break;
        }
        page += 1;
        tokio::time::sleep(delay).await;
    }
    Ok(stats)
}

// ---------- API ----------

/// GET /api/v1/import/lastfm/sync
pub async fn get_sync(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Value>> {
    let row: Option<(String, i64, Option<i64>, Option<String>)> =
        sqlx::query_as("SELECT username, synced_to, last_run_at, last_error FROM lastfm_sync WHERE user_id = ?")
            .bind(u.id)
            .fetch_optional(&st.db)
            .await?;
    let interval = st.cfg.lastfm.sync_interval_secs;
    Ok(Json(match row {
        Some((username, synced_to, last_run_at, last_error)) => json!({
            "enabled": true, "username": username, "synced_to": synced_to,
            "last_run_at": last_run_at, "last_error": last_error, "interval_secs": interval,
        }),
        None => json!({"enabled": false, "interval_secs": interval}),
    }))
}

#[derive(Deserialize, Default)]
pub struct SyncReq {
    /// Usuario de Last.fm. Por defecto, el de la cuenta conectada.
    pub username: Option<String>,
}

/// PUT /api/v1/import/lastfm/sync — activa la sincronización (o cambia el usuario de Last.fm).
///
/// Empieza desde la última escucha importada de Last.fm; si no hay ninguna, desde ahora (el
/// historial anterior se trae con «Importar todo de Last.fm»).
pub async fn put_sync(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    body: Option<Json<SyncReq>>,
) -> AppResult<Json<Value>> {
    if st.cfg.lastfm.sync_interval_secs == 0 {
        return Err(AppError::BadRequest("el administrador ha desactivado la sincronización con Last.fm".into()));
    }
    let req = body.map(|b| b.0).unwrap_or_default();
    let key = lastfm::key_for(&st, u.id)
        .await?
        .ok_or_else(|| AppError::BadRequest("falta configurar la API key de Last.fm (admin o tu propia key)".into()))?;
    let connected: Option<(Option<String>,)> =
        sqlx::query_as("SELECT remote_username FROM connections WHERE user_id = ? AND provider = 'lastfm'")
            .bind(u.id)
            .fetch_optional(&st.db)
            .await?;
    let user = match req.username.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => n.to_string(),
        None => connected
            .and_then(|c| c.0)
            .ok_or_else(|| AppError::BadRequest("conecta tu cuenta de Last.fm o indica el usuario".into()))?,
    };
    let user = match st.lastfm.user_exists(&key, &user).await {
        Ok(n) => n,
        Err(LfmError::NotFound(_)) => {
            return Err(AppError::BadRequest(format!("Last.fm no conoce al usuario «{user}»")));
        }
        Err(e) => return Err(AppError::BadGateway(e.to_string())),
    };
    let (last,): (Option<i64>,) =
        sqlx::query_as("SELECT MAX(played_at) FROM scrobbles WHERE user_id = ? AND source = 'lastfm'")
            .bind(u.id)
            .fetch_one(&st.db)
            .await?;
    let t = now();
    // Al cambiar de usuario de Last.fm se conserva el punto en el que iba; el filtro de duplicados cubre el solape.
    sqlx::query(
        "INSERT INTO lastfm_sync (user_id, username, synced_to, created_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(user_id) DO UPDATE SET username = excluded.username, last_error = NULL",
    )
    .bind(u.id)
    .bind(&user)
    .bind(last.unwrap_or(t))
    .bind(t)
    .execute(&st.db)
    .await?;
    st.lastfm_sync_wake.notify_one();
    get_sync(State(st), CurrentUser(u)).await
}

/// DELETE /api/v1/import/lastfm/sync
pub async fn delete_sync(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM lastfm_sync WHERE user_id = ?").bind(u.id).execute(&st.db).await?;
    Ok(Json(json!({"enabled": false})))
}
