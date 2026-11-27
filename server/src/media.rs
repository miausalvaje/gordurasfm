//! Proxy de imágenes: portadas de álbumes e imágenes de artistas, desde Navidrome (con las
//! credenciales del usuario que las pide) o desde Last.fm, guardadas en caché en disco.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::header,
    response::Response,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    auth::{CurrentUser, navidrome_creds},
    db::now,
    error::{AppError, AppResult},
    state::AppState,
};

/// Tiempo antes de volver a intentar una imagen que no existía.
const NEGATIVE_TTL: i64 = 86400;
const MAX_IMAGE: usize = 15 * 1024 * 1024;

#[derive(Deserialize)]
pub struct SizeQuery {
    pub size: Option<u32>,
}

fn norm_size(s: Option<u32>) -> u32 {
    match s.unwrap_or(300) {
        0..=64 => 64,
        65..=150 => 150,
        151..=300 => 300,
        301..=600 => 600,
        _ => 1000,
    }
}

/// GET /api/v1/media/cover/{album_id}?size=300
pub async fn cover(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(id): Path<i64>,
    Query(q): Query<SizeQuery>,
) -> AppResult<Response> {
    // Solo portadas de álbumes que el usuario ha escuchado (o cualquiera si es admin).
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT al.cover_ref FROM albums al WHERE al.id = ? AND (? OR EXISTS (SELECT 1 FROM scrobbles s WHERE s.user_id = ? AND s.album_id = al.id))",
    )
    .bind(id)
    .bind(u.is_admin)
    .bind(u.id)
    .fetch_optional(&st.db)
    .await?;
    let r = row.and_then(|r| r.0).ok_or(AppError::NotFound)?;
    serve_ref(&st, u.id, &r, norm_size(q.size)).await
}

/// GET /api/v1/media/artist/{artist_id}?size=300
pub async fn artist(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(id): Path<i64>,
    Query(q): Query<SizeQuery>,
) -> AppResult<Response> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT ar.image_ref FROM artists ar WHERE ar.id = ? AND (? OR EXISTS (SELECT 1 FROM scrobbles s WHERE s.user_id = ? AND s.artist_id = ar.id))",
    )
    .bind(id)
    .bind(u.is_admin)
    .bind(u.id)
    .fetch_optional(&st.db)
    .await?;
    let r = row.and_then(|r| r.0).ok_or(AppError::NotFound)?;
    serve_ref(&st, u.id, &r, norm_size(q.size)).await
}

fn image_response(mime: &str, data: Vec<u8>) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "private, max-age=604800")
        .body(Body::from(data))
        .unwrap()
}

fn sniff_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(b"\x89PNG") {
        Some("image/png")
    } else if data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.starts_with(b"GIF8") {
        Some("image/gif")
    } else {
        None
    }
}

/// Sirve una imagen por referencia (`nd:<coverArt>` o `url:<https…>`), desde la caché o descargándola.
pub async fn serve_ref(st: &AppState, user_id: i64, r: &str, size: u32) -> AppResult<Response> {
    let cached: Option<(Option<String>, i64)> =
        sqlx::query_as("SELECT hash, fetched_at FROM image_refs WHERE ref = ? AND size = ?")
            .bind(r)
            .bind(size)
            .fetch_optional(&st.db)
            .await?;
    match &cached {
        Some((Some(hash), _)) => {
            if let Some(resp) = from_disk(st, hash).await? {
                return Ok(resp);
            }
        }
        Some((None, at)) if now() - at < NEGATIVE_TTL => return Err(AppError::NotFound),
        _ => {}
    }

    let fetched = fetch(st, user_id, r, size).await;
    let t = now();
    match fetched {
        // Fallo temporal (Navidrome caído, red, credenciales): no se recuerda como «sin portada».
        Fetched::Retry => Err(AppError::BadGateway("imagen no disponible ahora".into())),
        Fetched::Found(data) => {
            let Some(mime) = sniff_mime(&data) else {
                remember_missing(st, r, size, t).await?;
                return Err(AppError::NotFound);
            };
            let hash = hex::encode(Sha256::digest(&data));
            let dir = st.cfg.images_dir();
            tokio::fs::create_dir_all(&dir).await.map_err(anyhow::Error::from)?;
            let path = dir.join(&hash);
            if !path.exists() {
                tokio::fs::write(&path, &data).await.map_err(anyhow::Error::from)?;
            }
            sqlx::query("INSERT OR IGNORE INTO images (hash, mime, source, created_at) VALUES (?, ?, ?, ?)")
                .bind(&hash)
                .bind(mime)
                .bind(r)
                .bind(t)
                .execute(&st.db)
                .await?;
            sqlx::query("INSERT INTO image_refs (ref, size, hash, fetched_at) VALUES (?, ?, ?, ?) ON CONFLICT(ref, size) DO UPDATE SET hash = excluded.hash, fetched_at = excluded.fetched_at")
                .bind(r)
                .bind(size)
                .bind(&hash)
                .bind(t)
                .execute(&st.db)
                .await?;
            Ok(image_response(mime, data))
        }
        Fetched::Missing => {
            remember_missing(st, r, size, t).await?;
            Err(AppError::NotFound)
        }
    }
}

async fn remember_missing(st: &AppState, r: &str, size: u32, t: i64) -> AppResult<()> {
    sqlx::query("INSERT INTO image_refs (ref, size, hash, fetched_at) VALUES (?, ?, NULL, ?) ON CONFLICT(ref, size) DO UPDATE SET hash = NULL, fetched_at = excluded.fetched_at")
        .bind(r)
        .bind(size)
        .bind(t)
        .execute(&st.db)
        .await?;
    Ok(())
}

enum Fetched {
    Found(Vec<u8>),
    /// La imagen no existe: se recuerda un día para no insistir.
    Missing,
    /// Fallo pasajero: se reintentará en la próxima petición.
    Retry,
}

/// Lee una imagen guardada (por hash). La usa también el Wrapped.
pub async fn from_disk(st: &AppState, hash: &str) -> AppResult<Option<Response>> {
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(None);
    }
    let mime: Option<(String,)> =
        sqlx::query_as("SELECT mime FROM images WHERE hash = ?").bind(hash).fetch_optional(&st.db).await?;
    let Some((mime,)) = mime else { return Ok(None) };
    match tokio::fs::read(st.cfg.images_dir().join(hash)).await {
        Ok(data) => Ok(Some(image_response(&mime, data))),
        Err(_) => Ok(None),
    }
}

async fn fetch(st: &AppState, user_id: i64, r: &str, size: u32) -> Fetched {
    use crate::subsonic::SubsonicError;
    if let Some(cover) = r.strip_prefix("nd:") {
        let Ok(creds) = navidrome_creds(st, user_id).await else { return Fetched::Retry };
        let size = size.to_string();
        match st.subsonic.bytes(&creds, "getCoverArt", &[("id", cover), ("size", &size)]).await {
            Ok((_, b)) if b.len() <= MAX_IMAGE => Fetched::Found(b.to_vec()),
            Ok(_) => Fetched::Missing,
            Err(e) => {
                tracing::debug!(error = %e, referencia = r, "portada no disponible en Navidrome");
                match e {
                    SubsonicError::Api(_) => Fetched::Missing,
                    SubsonicError::BadCredentials | SubsonicError::Network(_) => Fetched::Retry,
                }
            }
        }
    } else if let Some(url) = r.strip_prefix("url:") {
        if !crate::scrobble::allowed_cover_url(url) {
            return Fetched::Missing;
        }
        // Last.fm sirve otros tamaños cambiando el segmento /300x300/.
        let url = if size >= 600 { url.replace("/300x300/", "/770x0/") } else { url.to_string() };
        let Ok(resp) = st.http.get(&url).send().await else { return Fetched::Retry };
        let status = resp.status();
        if !status.is_success() {
            // Last.fm responde a veces 403/404 a imágenes que existen cuando se piden muchas seguidas:
            // solo «410 Gone» se da por definitivo.
            return if status.as_u16() == 410 { Fetched::Missing } else { Fetched::Retry };
        }
        match resp.bytes().await {
            Ok(b) if b.len() <= MAX_IMAGE => Fetched::Found(b.to_vec()),
            Ok(_) => Fetched::Missing,
            Err(_) => Fetched::Retry,
        }
    } else {
        Fetched::Missing
    }
}

/// Guarda una copia propia de la imagen (para el Wrapped) y devuelve su hash. `None` si no existe.
pub async fn snapshot(st: &AppState, user_id: i64, r: &str, size: u32) -> Option<String> {
    serve_ref(st, user_id, r, size).await.ok()?;
    sqlx::query_as::<_, (Option<String>,)>("SELECT hash FROM image_refs WHERE ref = ? AND size = ?")
        .bind(r)
        .bind(size)
        .fetch_optional(&st.db)
        .await
        .ok()
        .flatten()
        .and_then(|(h,)| h)
}

/// GET /api/v1/media/stream/{song_id} — audio de Navidrome (para la música del Wrapped), con `Range`.
/// Solo canciones que el usuario ha escuchado.
pub async fn stream(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(song_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> AppResult<Response> {
    let allowed: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM track_navidrome tn WHERE tn.navidrome_song_id = ? \
         AND EXISTS (SELECT 1 FROM scrobbles s WHERE s.user_id = ? AND s.track_id = tn.track_id) LIMIT 1",
    )
    .bind(&song_id)
    .bind(u.id)
    .fetch_optional(&st.db)
    .await?;
    if allowed.is_none() {
        return Err(AppError::NotFound);
    }
    let creds = navidrome_creds(&st, u.id).await?;
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let resp = st
        .subsonic
        .stream(&creds, &[("id", &song_id), ("format", "mp3"), ("maxBitRate", "192")], range)
        .await
        .map_err(|e| {
        tracing::debug!(error = %e, "audio no disponible en Navidrome");
        AppError::NotFound
    })?;
    let mut b =
        Response::builder().status(resp.status().as_u16()).header(header::CACHE_CONTROL, "private, max-age=3600");
    for h in [header::CONTENT_TYPE, header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES] {
        if let Some(v) = resp.headers().get(h.as_str()).and_then(|v| v.to_str().ok()) {
            b = b.header(h, v);
        }
    }
    Ok(b.body(Body::from_stream(resp.bytes_stream())).unwrap())
}
