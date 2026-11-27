//! Enriquecedor: empareja álbumes, canciones y artistas con Navidrome (ids y portadas).
//!
//! Las escuchas de Navidrome no traen el id de la canción, así que se busca con `search3`
//! usando las credenciales de un usuario que la haya escuchado (respeta sus bibliotecas).

use std::time::Duration;

use serde_json::Value;

use crate::{
    db::now,
    normalize,
    state::AppState,
    subsonic::{Creds, SubsonicError},
};

const BATCH: i64 = 25;
/// Se vuelve a buscar lo que no se encontró pasada una semana.
const RECHECK_SECS: i64 = 7 * 86400;

pub async fn run(st: AppState) {
    if !st.cfg.enrich.enabled {
        return;
    }
    tracing::info!("enriquecedor en marcha");
    match recover_covers(&st).await {
        Ok(n) if n > 0 => tracing::info!(albumes = n, "portadas recuperadas de las escuchas guardadas"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "recuperando portadas"),
    }
    loop {
        loop {
            match pass(&st).await {
                Ok(n) if n > 0 => continue,
                Ok(_) => break,
                Err(e) => {
                    tracing::warn!(error = %e, "enriquecedor");
                    break;
                }
            }
        }
        tokio::select! {
            _ = st.enrich_wake.notified() => tokio::time::sleep(Duration::from_secs(2)).await,
            _ = tokio::time::sleep(Duration::from_secs(st.cfg.enrich.interval_secs)) => {}
        }
    }
}

/// Una pasada: hasta `BATCH` álbumes, canciones y artistas. Devuelve cuántos ha revisado.
pub async fn pass(st: &AppState) -> anyhow::Result<usize> {
    let mut n = 0;
    // Las búsquedas en Navidrome no bloquean; las escrituras van con turno (ver `write_lock`).
    n += albums(st).await?;
    n += tracks(st).await?;
    n += artists(st).await?;
    Ok(n)
}

/// Álbumes sin portada cuyas escuchas guardan una URL de imagen de Last.fm (por ejemplo, de cuando
/// el host de las imágenes aún no estaba admitido): se les pone esa portada.
async fn recover_covers(st: &AppState) -> anyhow::Result<usize> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT s.album_id, json_extract(s.raw, '$.image') FROM scrobbles s JOIN albums al ON al.id = s.album_id \
         WHERE al.cover_ref IS NULL AND json_extract(s.raw, '$.image') IS NOT NULL GROUP BY s.album_id",
    )
    .fetch_all(&st.db)
    .await?;
    let mut n = 0;
    for (album_id, url) in rows {
        if !crate::scrobble::allowed_cover_url(&url) {
            continue;
        }
        let _turno = st.write_lock.lock().await;
        let r = sqlx::query("UPDATE albums SET cover_ref = ? WHERE id = ? AND cover_ref IS NULL")
            .bind(format!("url:{url}"))
            .bind(album_id)
            .execute(&st.db)
            .await?;
        n += r.rows_affected() as usize;
    }
    Ok(n)
}

async fn creds_for(st: &AppState, sql_user: &str, id: i64) -> anyhow::Result<Option<Creds>> {
    // Un usuario con credenciales válidas que haya escuchado ese elemento.
    let sql = format!(
        "SELECT nc.user_id FROM navidrome_credentials nc WHERE nc.status = 'ok' AND EXISTS \
         (SELECT 1 FROM scrobbles s WHERE s.user_id = nc.user_id AND s.{sql_user} = ?) LIMIT 1"
    );
    let uid: Option<(i64,)> = sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(id).fetch_optional(&st.db).await?;
    match uid {
        Some((u,)) => match crate::auth::navidrome_creds(st, u).await {
            Ok(c) => Ok(Some(c)),
            Err(_) => Ok(None),
        },
        None => Ok(None),
    }
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Navidrome dice que las credenciales ya no valen: se marcan para no insistir.
async fn on_error(st: &AppState, creds: &Creds, e: SubsonicError) -> anyhow::Result<()> {
    if matches!(e, SubsonicError::BadCredentials) {
        sqlx::query("UPDATE navidrome_credentials SET status = 'invalid', updated_at = ? WHERE user_id = (SELECT id FROM users WHERE username = ?)")
            .bind(now())
            .bind(&creds.u)
            .execute(&st.db)
            .await?;
        tracing::warn!(usuario = %creds.u, "Navidrome rechaza las credenciales guardadas; tendrá que volver a entrar");
        return Ok(());
    }
    Err(anyhow::anyhow!("{e}"))
}

async fn pause(st: &AppState) {
    tokio::time::sleep(Duration::from_millis(st.cfg.enrich.delay_ms)).await;
}

async fn albums(st: &AppState) -> anyhow::Result<usize> {
    let t = now();
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT al.id, al.title, al.title_key, ar.name_key FROM albums al JOIN artists ar ON ar.id = al.artist_id \
         WHERE (al.cover_ref IS NULL OR al.cover_ref LIKE 'url:%' OR al.navidrome_id IS NULL) \
           AND (al.nd_checked_at IS NULL OR al.nd_checked_at < ?) LIMIT ?",
    )
    .bind(t - RECHECK_SECS)
    .bind(BATCH)
    .fetch_all(&st.db)
    .await?;
    for (id, title, title_key, artist_key) in &rows {
        {
            let _turno = st.write_lock.lock().await;
            sqlx::query("UPDATE albums SET nd_checked_at = ? WHERE id = ?").bind(t).bind(id).execute(&st.db).await?;
        }
        let Some(creds) = creds_for(st, "album_id", *id).await? else { continue };
        let found = match st.subsonic.search3(&creds, title, 0, 20, 0).await {
            Ok((_, albums, _)) => albums,
            Err(e) => {
                on_error(st, &creds, e).await?;
                continue;
            }
        };
        let _turno = st.write_lock.lock().await;
        let hit = found.iter().find(|a| {
            normalize::key(s(a, "name")) == *title_key
                && (normalize::key(s(a, "artist")) == *artist_key
                    || normalize::key(s(a, "displayArtist")) == *artist_key)
        });
        if let Some(a) = hit {
            let cover = a.get("coverArt").and_then(Value::as_str).map(|c| format!("nd:{c}"));
            sqlx::query("UPDATE albums SET navidrome_id = ?, cover_ref = COALESCE(?, cover_ref) WHERE id = ?")
                .bind(s(a, "id"))
                .bind(cover)
                .bind(id)
                .execute(&st.db)
                .await?;
        }
        drop(_turno);
        pause(st).await;
    }
    Ok(rows.len())
}

async fn tracks(st: &AppState) -> anyhow::Result<usize> {
    let t = now();
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT tr.id, tr.title, tr.title_key, ar.name_key FROM tracks tr JOIN artists ar ON ar.id = tr.artist_id \
         WHERE (tr.nd_checked_at IS NULL OR (tr.nd_checked_at < ? AND NOT EXISTS (SELECT 1 FROM track_navidrome tn WHERE tn.track_id = tr.id))) \
         LIMIT ?",
    )
    .bind(t - RECHECK_SECS)
    .bind(BATCH)
    .fetch_all(&st.db)
    .await?;
    for (id, title, title_key, artist_key) in &rows {
        {
            let _turno = st.write_lock.lock().await;
            sqlx::query("UPDATE tracks SET nd_checked_at = ? WHERE id = ?").bind(t).bind(id).execute(&st.db).await?;
        }
        let Some(creds) = creds_for(st, "track_id", *id).await? else { continue };
        let songs = match st.subsonic.search3(&creds, title, 0, 0, 20).await {
            Ok((_, _, songs)) => songs,
            Err(e) => {
                on_error(st, &creds, e).await?;
                continue;
            }
        };
        let _turno = st.write_lock.lock().await;
        for song in songs.iter().filter(|x| normalize::key(s(x, "title")) == *title_key) {
            let artist_ok = normalize::key(s(song, "artist")) == *artist_key
                || normalize::key(s(song, "displayArtist")) == *artist_key;
            if !artist_ok {
                continue;
            }
            // ¿Con cuál de nuestros álbumes de esta canción coincide?
            let album: Option<(i64, Option<String>)> = sqlx::query_as(
                "SELECT DISTINCT al.id, al.cover_ref FROM albums al JOIN scrobbles sc ON sc.album_id = al.id \
                 WHERE sc.track_id = ? AND al.title_key = ? LIMIT 1",
            )
            .bind(id)
            .bind(normalize::key(s(song, "album")))
            .fetch_optional(&st.db)
            .await?;
            if let Some((album_id, cover_ref)) = album {
                sqlx::query(
                    "INSERT OR IGNORE INTO track_navidrome (track_id, album_id, navidrome_song_id) VALUES (?, ?, ?)",
                )
                .bind(id)
                .bind(album_id)
                .bind(s(song, "id"))
                .execute(&st.db)
                .await?;
                // De paso, la portada del álbum si todavía no tiene la de Navidrome.
                if !cover_ref.as_deref().unwrap_or("").starts_with("nd:")
                    && let Some(c) = song.get("coverArt").and_then(Value::as_str)
                {
                    sqlx::query(
                        "UPDATE albums SET cover_ref = ?, navidrome_id = COALESCE(navidrome_id, ?) WHERE id = ?",
                    )
                    .bind(format!("nd:{c}"))
                    .bind(song.get("albumId").and_then(Value::as_str))
                    .bind(album_id)
                    .execute(&st.db)
                    .await?;
                }
            }
            if let Some(d) = song.get("duration").and_then(Value::as_i64) {
                sqlx::query("UPDATE tracks SET duration_ms = COALESCE(duration_ms, ?) WHERE id = ?")
                    .bind(d * 1000)
                    .bind(id)
                    .execute(&st.db)
                    .await?;
            }
        }
        drop(_turno);
        pause(st).await;
    }
    Ok(rows.len())
}

async fn artists(st: &AppState) -> anyhow::Result<usize> {
    let t = now();
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, name, name_key FROM artists WHERE image_ref IS NULL AND (nd_checked_at IS NULL OR nd_checked_at < ?) LIMIT ?",
    )
    .bind(t - RECHECK_SECS)
    .bind(BATCH)
    .fetch_all(&st.db)
    .await?;
    for (id, name, key) in &rows {
        {
            let _turno = st.write_lock.lock().await;
            sqlx::query("UPDATE artists SET nd_checked_at = ? WHERE id = ?").bind(t).bind(id).execute(&st.db).await?;
        }
        let Some(creds) = creds_for(st, "artist_id", *id).await? else { continue };
        let found = match st.subsonic.search3(&creds, name, 10, 0, 0).await {
            Ok((a, _, _)) => a,
            Err(e) => {
                on_error(st, &creds, e).await?;
                continue;
            }
        };
        let _turno = st.write_lock.lock().await;
        if let Some(a) = found.iter().find(|a| normalize::key(s(a, "name")) == *key) {
            let nd_id = s(a, "id");
            let img =
                a.get("coverArt").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("ar-{nd_id}"));
            sqlx::query("UPDATE artists SET navidrome_id = ?, image_ref = ? WHERE id = ?")
                .bind(nd_id)
                .bind(format!("nd:{img}"))
                .bind(id)
                .execute(&st.db)
                .await?;
        }
        drop(_turno);
        pause(st).await;
    }
    Ok(rows.len())
}
