//! Reenvío de escuchas a ListenBrainz (y más adelante Last.fm) con cola persistente y reintentos.

use std::time::Duration;

use serde_json::{Value, json};

use crate::{db::now, listenbrainz::client::SubmitError, state::AppState};

const BATCH: i64 = 100;

/// Bucle del reenviador: se despierta con cada escucha nueva o cada `poll_secs`.
pub async fn run(st: AppState) {
    tracing::info!("reenviador en marcha");
    loop {
        loop {
            match process_due(&st).await {
                Ok(true) => continue, // queda trabajo
                Ok(false) => break,
                Err(e) => {
                    tracing::error!(error = %e, "reenviador");
                    break;
                }
            }
        }
        tokio::select! {
            _ = st.forward_wake.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(st.cfg.forward.poll_secs)) => {}
        }
    }
}

/// Espera antes del siguiente intento: base · 2^(intentos−1), con tope y algo de azar.
pub fn backoff(base: i64, max: i64, attempts: i64) -> i64 {
    if base <= 0 {
        return 0;
    }
    let exp = (attempts - 1).clamp(0, 20) as u32;
    let d = base.saturating_mul(1i64 << exp).min(max);
    d + (rand::random::<u64>() % (d as u64 / 4 + 1)) as i64
}

/// Procesa un lote por cada (usuario, destino) con escuchas pendientes y vencidas.
/// Devuelve `true` si algún lote salió lleno y bien (probablemente quede más).
pub async fn process_due(st: &AppState) -> anyhow::Result<bool> {
    let t = now();
    let groups: Vec<(i64, String)> = sqlx::query_as(
        "SELECT DISTINCT user_id, target FROM forward_queue WHERE status = 'pending' AND next_attempt_at <= ? LIMIT 50",
    )
    .bind(t)
    .fetch_all(&st.db)
    .await?;
    let mut more = false;
    for (user_id, target) in groups {
        match target.as_str() {
            "listenbrainz" => more |= forward_listenbrainz(st, user_id).await?,
            "lastfm" => more |= forward_lastfm(st, user_id).await?,
            other => tracing::debug!(destino = other, "destino desconocido"),
        }
    }
    Ok(more)
}

#[derive(sqlx::FromRow)]
struct Row {
    qid: i64,
    attempts: i64,
    played_at: i64,
    raw: Option<String>,
    artist: String,
    track: String,
    album: Option<String>,
    album_artist: Option<String>,
    duration_ms: Option<i64>,
}

/// Token de ListenBrainz descifrado si la conexión existe, está activa y bien.
async fn lb_token(st: &AppState, user_id: i64) -> anyhow::Result<Option<String>> {
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT secret_enc FROM connections WHERE user_id = ? AND provider = 'listenbrainz' AND forward_enabled = 1 AND status = 'ok'",
    )
    .bind(user_id)
    .fetch_optional(&st.db)
    .await?;
    Ok(match row {
        Some((blob,)) => Some(st.crypto.decrypt_str(&blob)?),
        None => None,
    })
}

async fn forward_listenbrainz(st: &AppState, user_id: i64) -> anyhow::Result<bool> {
    let t = now();
    let Some(token) = lb_token(st, user_id).await? else {
        // Sin conexión válida: se deja en pausa hasta que se reconecte o se reactive.
        sqlx::query("UPDATE forward_queue SET status = 'paused', updated_at = ? WHERE user_id = ? AND target = 'listenbrainz' AND status = 'pending'")
            .bind(t)
            .bind(user_id)
            .execute(&st.db)
            .await?;
        return Ok(false);
    };
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT q.id AS qid, q.attempts, s.played_at, s.raw, a.name AS artist, tr.title AS track, al.title AS album, \
                aa.name AS album_artist, COALESCE(s.duration_ms, tr.duration_ms) AS duration_ms \
         FROM forward_queue q JOIN scrobbles s ON s.id = q.scrobble_id \
         JOIN artists a ON a.id = s.artist_id JOIN tracks tr ON tr.id = s.track_id LEFT JOIN albums al ON al.id = s.album_id \
         LEFT JOIN artists aa ON aa.id = al.artist_id \
         WHERE q.user_id = ? AND q.target = ? AND q.status = 'pending' AND q.next_attempt_at <= ? \
         ORDER BY s.played_at LIMIT ?",
    )
    .bind(user_id)
    .bind("listenbrainz")
    .bind(t)
    .bind(BATCH)
    .fetch_all(&st.db)
    .await?;
    if rows.is_empty() {
        return Ok(false);
    }

    let payload: Vec<Value> = rows.iter().map(lb_listen).collect();
    let listen_type = if rows.len() == 1 { "single" } else { "import" };
    match st.lb.submit(&token, listen_type, payload).await {
        Ok(()) => {
            let ids: Vec<i64> = rows.iter().map(|r| r.qid).collect();
            mark_done(st, ids).await?;
            tracing::info!(usuario = user_id, enviadas = rows.len(), "reenviadas a ListenBrainz");
            Ok(rows.len() as i64 == BATCH)
        }
        Err(SubmitError::Permanent(msg)) if rows.len() > 1 => {
            // Una escucha mala no debe bloquear el lote: se mandan de una en una.
            tracing::warn!(error = %msg, "lote rechazado; se reintenta de una en una");
            for r in &rows {
                match st.lb.submit(&token, "single", vec![lb_listen(r)]).await {
                    Ok(()) => mark_done(st, [r.qid]).await?,
                    Err(e) => apply_error(st, user_id, &[r], e).await?,
                }
            }
            Ok(false)
        }
        Err(e) => {
            let refs: Vec<&Row> = rows.iter().collect();
            apply_error(st, user_id, &refs, e).await?;
            Ok(false)
        }
    }
}

async fn due_rows(st: &AppState, user_id: i64, target: &str, limit: i64) -> sqlx::Result<Vec<Row>> {
    sqlx::query_as(
        "SELECT q.id AS qid, q.attempts, s.played_at, s.raw, a.name AS artist, tr.title AS track, al.title AS album, \
                aa.name AS album_artist, COALESCE(s.duration_ms, tr.duration_ms) AS duration_ms \
         FROM forward_queue q JOIN scrobbles s ON s.id = q.scrobble_id \
         JOIN artists a ON a.id = s.artist_id JOIN tracks tr ON tr.id = s.track_id LEFT JOIN albums al ON al.id = s.album_id \
         LEFT JOIN artists aa ON aa.id = al.artist_id \
         WHERE q.user_id = ? AND q.target = ? AND q.status = 'pending' AND q.next_attempt_at <= ? \
         ORDER BY s.played_at LIMIT ?",
    )
    .bind(user_id)
    .bind(target)
    .bind(now())
    .bind(limit)
    .fetch_all(&st.db)
    .await
}

/// Last.fm rechaza escuchas de hace más de 14 días.
pub const LASTFM_MAX_AGE_SECS: i64 = 14 * 86400;
const LASTFM_BATCH: i64 = 50;

async fn set_rows(
    st: &AppState,
    ids: &[i64],
    status: &str,
    error: Option<&str>,
    next: Option<i64>,
) -> sqlx::Result<()> {
    let t = now();
    let mut tx = st.db.begin().await?;
    for id in ids {
        sqlx::query(
            "UPDATE forward_queue SET status = ?, attempts = attempts + 1, last_error = ?, \
             next_attempt_at = COALESCE(?, next_attempt_at), updated_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(error)
        .bind(next)
        .bind(t)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

async fn forward_lastfm(st: &AppState, user_id: i64) -> anyhow::Result<bool> {
    use crate::lastfm::client::{LfmError, ScrobbleItem};
    let t = now();
    let Some((key, sk)) = crate::lastfm::session_for(st, user_id, true).await? else {
        sqlx::query("UPDATE forward_queue SET status = 'paused', updated_at = ? WHERE user_id = ? AND target = 'lastfm' AND status = 'pending'")
            .bind(t)
            .bind(user_id)
            .execute(&st.db)
            .await?;
        return Ok(false);
    };
    let rows = due_rows(st, user_id, "lastfm", LASTFM_BATCH).await?;
    if rows.is_empty() {
        return Ok(false);
    }
    // Las demasiado viejas no se pueden enviar: se marcan como perdidas para Last.fm.
    let (old, fresh): (Vec<&Row>, Vec<&Row>) = rows.iter().partition(|r| r.played_at < t - LASTFM_MAX_AGE_SECS + 60);
    if !old.is_empty() {
        let ids: Vec<i64> = old.iter().map(|r| r.qid).collect();
        set_rows(st, &ids, "dead", Some("Last.fm no acepta escuchas de hace más de 14 días"), None).await?;
    }
    if fresh.is_empty() {
        return Ok(true);
    }
    let items: Vec<ScrobbleItem> = fresh
        .iter()
        .map(|r| ScrobbleItem {
            artist: r.artist.clone(),
            track: r.track.clone(),
            album: r.album.clone(),
            album_artist: r.album_artist.clone().filter(|aa| aa != &r.artist),
            timestamp: r.played_at,
            duration_secs: r.duration_ms.map(|d| d / 1000),
        })
        .collect();
    match st.lastfm.scrobble(&key, &sk, &items).await {
        Ok(results) => {
            let mut done = vec![];
            for (r, res) in fresh.iter().zip(results) {
                match res {
                    Ok(()) => done.push(r.qid),
                    // 3 = demasiado vieja, 4 = en el futuro: no tiene arreglo.
                    Err((c @ (3 | 4), m)) => {
                        set_rows(st, &[r.qid], "dead", Some(&format!("ignorada ({c}): {m}")), None).await?
                    }
                    // 5 = límite diario de escuchas: mañana.
                    Err((5, m)) => {
                        set_rows(st, &[r.qid], "pending", Some(&format!("límite diario: {m}")), Some(t + 3600)).await?
                    }
                    Err((c, m)) => {
                        set_rows(st, &[r.qid], "failed", Some(&format!("ignorada ({c}): {m}")), None).await?
                    }
                }
            }
            if !done.is_empty() {
                mark_done(st, done.clone()).await?;
                tracing::info!(usuario = user_id, enviadas = done.len(), "reenviadas a Last.fm");
            }
            Ok(rows.len() as i64 == LASTFM_BATCH)
        }
        Err(LfmError::Auth(m)) => {
            let ids: Vec<i64> = fresh.iter().map(|r| r.qid).collect();
            let mut tx = st.db.begin().await?;
            for id in &ids {
                sqlx::query("UPDATE forward_queue SET status = 'paused', last_error = ?, updated_at = ? WHERE id = ?")
                    .bind(&m)
                    .bind(t)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query("UPDATE connections SET status = 'auth_failed', last_error = ?, updated_at = ? WHERE user_id = ? AND provider = 'lastfm'")
                .bind(&m)
                .bind(t)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            tracing::warn!(usuario = user_id, error = %m, "Last.fm rechaza la sesión; hay que reconectar");
            Ok(false)
        }
        Err(e @ (LfmError::Retry(_) | LfmError::NotFound(_) | LfmError::Permanent(_))) => {
            let cfg = &st.cfg.forward;
            let mut tx = st.db.begin().await?;
            for r in &fresh {
                let attempts = r.attempts + 1;
                let wait = backoff(cfg.base_delay_secs, cfg.max_delay_secs, attempts);
                sqlx::query("UPDATE forward_queue SET attempts = ?, next_attempt_at = ?, last_error = ?, updated_at = ? WHERE id = ?")
                    .bind(attempts)
                    .bind(t + wait)
                    .bind(e.to_string())
                    .bind(t)
                    .bind(r.qid)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await?;
            tracing::warn!(usuario = user_id, error = %e, "Last.fm no disponible; se reintentará");
            Ok(false)
        }
    }
}

/// Escucha en formato ListenBrainz: el `track_metadata` original de Navidrome si lo hay.
fn lb_listen(r: &Row) -> Value {
    let original = r
        .raw
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.get("track_metadata").cloned());
    let md = original.unwrap_or_else(|| {
        let mut info =
            json!({"submission_client": "GordurasFM", "submission_client_version": env!("CARGO_PKG_VERSION")});
        if let Some(d) = r.duration_ms {
            info["duration_ms"] = json!(d);
        }
        let mut md = json!({"artist_name": r.artist, "track_name": r.track, "additional_info": info});
        if let Some(a) = &r.album {
            md["release_name"] = json!(a);
        }
        md
    });
    json!({"listened_at": r.played_at, "track_metadata": md})
}

async fn mark_done(st: &AppState, ids: impl IntoIterator<Item = i64>) -> sqlx::Result<()> {
    let t = now();
    let mut tx = st.db.begin().await?;
    for id in ids {
        sqlx::query("UPDATE forward_queue SET status = 'done', attempts = attempts + 1, last_error = NULL, updated_at = ? WHERE id = ?")
            .bind(t)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

async fn apply_error(st: &AppState, user_id: i64, rows: &[&Row], e: SubmitError) -> anyhow::Result<()> {
    let t = now();
    let cfg = &st.cfg.forward;
    let mut tx = st.db.begin().await?;
    match &e {
        SubmitError::RetryLater { after, msg } => {
            for r in rows {
                let attempts = r.attempts + 1;
                let wait = after.unwrap_or(0).max(backoff(cfg.base_delay_secs, cfg.max_delay_secs, attempts));
                sqlx::query("UPDATE forward_queue SET attempts = ?, next_attempt_at = ?, last_error = ?, updated_at = ? WHERE id = ?")
                    .bind(attempts)
                    .bind(t + wait)
                    .bind(msg)
                    .bind(t)
                    .bind(r.qid)
                    .execute(&mut *tx)
                    .await?;
            }
            tracing::warn!(usuario = user_id, error = %msg, "ListenBrainz no disponible; se reintentará");
        }
        SubmitError::Auth(msg) => {
            for r in rows {
                sqlx::query("UPDATE forward_queue SET status = 'paused', last_error = ?, updated_at = ? WHERE id = ?")
                    .bind(msg)
                    .bind(t)
                    .bind(r.qid)
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query("UPDATE connections SET status = 'auth_failed', last_error = ?, updated_at = ? WHERE user_id = ? AND provider = 'listenbrainz'")
                .bind(msg)
                .bind(t)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            tracing::warn!(usuario = user_id, error = %msg, "ListenBrainz rechaza el token; hay que reconectar");
        }
        SubmitError::Permanent(msg) => {
            for r in rows {
                sqlx::query("UPDATE forward_queue SET status = 'failed', attempts = attempts + 1, last_error = ?, updated_at = ? WHERE id = ?")
                    .bind(msg)
                    .bind(t)
                    .bind(r.qid)
                    .execute(&mut *tx)
                    .await?;
            }
            tracing::warn!(usuario = user_id, error = %msg, "ListenBrainz rechaza la escucha");
        }
    }
    tx.commit().await?;
    Ok(())
}

/// «Escuchando ahora»: se reenvía al momento, sin cola (no tiene sentido reintentarlo).
pub fn spawn_now_playing(st: AppState, user_id: i64, item: Value) {
    tokio::spawn(async move {
        let token = match lb_token(&st, user_id).await {
            Ok(Some(t)) => t,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!(error = %e, "playing_now: token");
                return;
            }
        };
        let md = item.get("track_metadata").cloned().unwrap_or(Value::Null);
        if let Err(e) = st.lb.submit(&token, "playing_now", vec![json!({"track_metadata": md})]).await {
            tracing::debug!(error = ?e, "playing_now no reenviado");
        }
    });
}

/// «Escuchando ahora» en Last.fm, si el usuario tiene el reenvío activado.
pub fn spawn_now_playing_lastfm(st: AppState, user_id: i64, l: crate::scrobble::Listen) {
    tokio::spawn(async move {
        let Ok(Some((key, sk))) = crate::lastfm::session_for(&st, user_id, true).await else { return };
        let it = crate::lastfm::client::ScrobbleItem {
            artist: l.artist,
            track: l.track,
            album: l.album,
            album_artist: None,
            timestamp: now(),
            duration_secs: l.duration_ms.map(|d| d / 1000),
        };
        if let Err(e) = st.lastfm.now_playing(&key, &sk, &it).await {
            tracing::debug!(error = %e, "now playing de Last.fm no enviado");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::backoff;

    #[test]
    fn espera_creciente_con_tope() {
        for _ in 0..50 {
            let a = backoff(30, 21600, 1);
            assert!((30..=37).contains(&a), "{a}");
            let c = backoff(30, 21600, 3);
            assert!((120..=150).contains(&c), "{c}");
            let z = backoff(30, 21600, 40);
            assert!((21600..=27000).contains(&z), "{z}");
        }
        assert_eq!(backoff(0, 100, 5), 0);
    }
}
