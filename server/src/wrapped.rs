//! Wrapped: resumen anual guardado como «foto fija» en JSON (nombres, recuentos y copias propias de
//! las imágenes), para que no cambie aunque cambie la biblioteca.
//!
//! - Año en curso: vista previa (`version = 0`, `is_final = 0`), que se rehace como mucho cada hora.
//! - Años pasados: definitivo (`version >= 1`, `is_final = 1`). Se genera solo el 1 de enero a las 06:00
//!   (hora del usuario) o al pedirlo. «Rehacer» crea una versión nueva y conserva las anteriores.

use std::time::Duration;

use axum::{
    Json,
    extract::{Path, State},
    response::Response,
};
use chrono::{Datelike, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    auth::{CurrentUser, User},
    db::{Db, now},
    error::{AppError, AppResult},
    media, periods,
    state::AppState,
    stats::{self, user_tz},
};

/// Versión del formato del JSON.
pub const SCHEMA: i64 = 1;
/// Elementos de cada top (y opciones de cada prueba).
const TOP_N: i64 = 5;
/// Tamaño de las imágenes guardadas.
const IMG_SIZE: u32 = 600;
/// Tiempo mínimo entre dos vistas previas del año en curso.
const PREVIEW_TTL: i64 = 3600;
/// Canciones de fondo como máximo.
const SOUNDTRACK_N: usize = 12;
/// Hora local (1 de enero) a partir de la que se genera el definitivo del año anterior.
const FINAL_HOUR: u32 = 6;

pub const QUESTIONS: [&str; 3] = ["artist", "album", "track"];

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct WrappedRow {
    pub id: i64,
    pub year: i64,
    pub version: i64,
    pub is_final: bool,
    pub generated_at: i64,
    #[serde(skip)]
    pub data: String,
}

fn img(hash: Option<String>) -> Value {
    hash.map(|h| Value::String(format!("img:{h}"))).unwrap_or(Value::Null)
}

async fn album_cover_ref(db: &Db, album_id: i64) -> Option<String> {
    sqlx::query_as::<_, (Option<String>,)>("SELECT cover_ref FROM albums WHERE id = ?")
        .bind(album_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.0)
}

async fn snap_album(st: &AppState, user_id: i64, album_id: Option<i64>) -> Value {
    let Some(id) = album_id else { return Value::Null };
    match album_cover_ref(&st.db, id).await {
        Some(r) => img(media::snapshot(st, user_id, &r, IMG_SIZE).await),
        None => Value::Null,
    }
}

/// Imagen del artista (si Navidrome la tiene) o, si no, hasta 4 portadas de sus álbumes para un collage.
async fn snap_artist(st: &AppState, user_id: i64, artist_id: i64, from: i64, to: i64) -> (Value, Vec<Value>) {
    let image_ref: Option<String> =
        sqlx::query_as::<_, (Option<String>,)>("SELECT image_ref FROM artists WHERE id = ?")
            .bind(artist_id)
            .fetch_optional(&st.db)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.0);
    if let Some(r) = image_ref
        && let Some(h) = media::snapshot(st, user_id, &r, IMG_SIZE).await
    {
        return (img(Some(h)), vec![]);
    }
    let albums: Vec<(i64,)> = sqlx::query_as(
        "SELECT s.album_id FROM scrobbles s JOIN albums al ON al.id = s.album_id \
         WHERE s.user_id = ? AND s.artist_id = ? AND s.played_at >= ? AND s.played_at < ? AND al.cover_ref IS NOT NULL \
         GROUP BY s.album_id ORDER BY COUNT(*) DESC LIMIT 4",
    )
    .bind(user_id)
    .bind(artist_id)
    .bind(from)
    .bind(to)
    .fetch_all(&st.db)
    .await
    .unwrap_or_default();
    let mut covers = vec![];
    for (id,) in albums {
        let v = snap_album(st, user_id, Some(id)).await;
        if !v.is_null() {
            covers.push(v);
        }
    }
    (Value::Null, covers)
}

/// Artista más escuchado en un rango.
async fn top_artist_in(db: &Db, user_id: i64, from: i64, to: i64, n: i64) -> sqlx::Result<Vec<(i64, String, i64)>> {
    sqlx::query_as(
        "SELECT ar.id, ar.name, t.c FROM (SELECT artist_id, COUNT(*) AS c, MAX(played_at) AS last FROM scrobbles \
         WHERE user_id = ? AND played_at >= ? AND played_at < ? GROUP BY artist_id ORDER BY c DESC, last DESC LIMIT ?) t \
         JOIN artists ar ON ar.id = t.artist_id ORDER BY t.c DESC, t.last DESC",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(n)
    .fetch_all(db)
    .await
}

/// Calcula el JSON completo del Wrapped de `year`. `None` si ese año no tiene escuchas.
pub async fn build(st: &AppState, user: &User, year: i32) -> AppResult<Option<Value>> {
    let tz: Tz = user_tz(&user.timezone);
    let p = periods::year(tz, year, Utc::now());
    let (chart, totals) = stats::chart_and_totals(&st.db, user.id, &p, tz, now()).await?;
    if totals.plays == 0 {
        return Ok(None);
    }
    let (from, to) = (p.from, p.to);

    // Tops con copia de las imágenes.
    let mut top_artists = vec![];
    for a in stats::top_artists(&st.db, user.id, from, to, TOP_N, 0).await? {
        let (image, covers) = snap_artist(st, user.id, a.id, from, to).await;
        top_artists.push(json!({"rank": a.rank, "name": a.name, "plays": a.plays, "image": image, "covers": covers}));
    }
    let mut top_albums = vec![];
    for a in stats::top_albums(&st.db, user.id, from, to, TOP_N, 0).await? {
        let cover = snap_album(st, user.id, Some(a.id)).await;
        top_albums
            .push(json!({"rank": a.rank, "title": a.title, "artist": a.artist, "plays": a.plays, "cover": cover}));
    }
    let mut top_tracks = vec![];
    for t in stats::top_tracks(&st.db, user.id, from, to, TOP_N, 0).await? {
        let cover = snap_album(st, user.id, t.album_id).await;
        top_tracks.push(json!({
            "rank": t.rank, "title": t.title, "artist": t.artist, "album": t.album, "plays": t.plays, "cover": cover,
        }));
    }

    // Meses: escuchas y artista más escuchado de cada uno.
    let mut monthly = vec![];
    for (i, b) in chart.iter().enumerate() {
        let top = if b.plays > 0 { top_artist_in(&st.db, user.id, b.start, b.end, 1).await? } else { vec![] };
        monthly.push(json!({
            "month": i + 1,
            "plays": b.plays,
            "top_artist": top.first().map(|t| t.1.clone()),
        }));
    }
    let top_month = chart
        .iter()
        .enumerate()
        .filter(|(_, b)| b.plays > 0)
        .max_by(|a, b| a.1.plays.cmp(&b.1.plays).then(b.0.cmp(&a.0)))
        .map(|(i, b)| (i, b.start, b.end, b.plays));
    let top_month = match top_month {
        Some((i, start, end, plays)) => {
            let artist = top_artist_in(&st.db, user.id, start, end, 1).await?;
            let track = stats::top_tracks(&st.db, user.id, start, end, 1, 0).await?;
            let artist_json = match artist.first() {
                Some((id, name, c)) => {
                    let (image, covers) = snap_artist(st, user.id, *id, start, end).await;
                    json!({"name": name, "plays": c, "image": image, "covers": covers})
                }
                None => Value::Null,
            };
            let track_json = match track.first() {
                Some(t) => json!({"title": t.title, "artist": t.artist, "plays": t.plays,
                                  "cover": snap_album(st, user.id, t.album_id).await}),
                None => Value::Null,
            };
            json!({"month": i + 1, "plays": plays, "top_artist": artist_json, "top_track": track_json})
        }
        None => Value::Null,
    };

    // Evolución: los 3 artistas más escuchados de cada trimestre.
    let mut evolution = vec![];
    for q in 0..4 {
        let (s, e) = (chart[q * 3].start, chart[q * 3 + 2].end);
        let tops = top_artist_in(&st.db, user.id, s, e, 3).await?;
        let plays: i64 = chart[q * 3..q * 3 + 3].iter().map(|b| b.plays).sum();
        let image = match tops.first() {
            Some((id, ..)) => snap_artist(st, user.id, *id, s, e).await,
            None => (Value::Null, vec![]),
        };
        evolution.push(json!({
            "quarter": q + 1,
            "plays": plays,
            "top_artists": tops.iter().map(|t| json!({"name": t.1, "plays": t.2})).collect::<Vec<_>>(),
            "image": image.0,
            "covers": image.1,
        }));
    }

    // Música de fondo: canciones más escuchadas que existen en Navidrome.
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT (SELECT tn.navidrome_song_id FROM track_navidrome tn WHERE tn.track_id = t.track_id LIMIT 1) AS song, \
                tr.title, ar.name \
         FROM (SELECT track_id, COUNT(*) AS c, MAX(played_at) AS last FROM scrobbles \
               WHERE user_id = ? AND played_at >= ? AND played_at < ? GROUP BY track_id ORDER BY c DESC, last DESC LIMIT 60) t \
         JOIN tracks tr ON tr.id = t.track_id JOIN artists ar ON ar.id = tr.artist_id \
         WHERE song IS NOT NULL ORDER BY t.c DESC, t.last DESC",
    )
    .bind(user.id)
    .bind(from)
    .bind(to)
    .fetch_all(&st.db)
    .await?;
    let soundtrack: Vec<Value> = rows
        .into_iter()
        .take(SOUNDTRACK_N)
        .map(|(id, title, artist)| json!({"navidrome_song_id": id, "title": title, "artist": artist}))
        .collect();

    let days_with_music: (i64,) = {
        // Días distintos (hora local) con alguna escucha, a partir de la gráfica por horas.
        let offset_rows: Vec<(i64,)> = sqlx::query_as(
            "SELECT DISTINCT played_at / 3600 FROM scrobbles WHERE user_id = ? AND played_at >= ? AND played_at < ?",
        )
        .bind(user.id)
        .bind(from)
        .bind(to)
        .fetch_all(&st.db)
        .await?;
        let mut days = std::collections::HashSet::new();
        for (h,) in offset_rows {
            days.insert(periods::local_date(&tz, h * 3600));
        }
        (days.len() as i64,)
    };
    let (first_play,): (Option<i64>,) =
        sqlx::query_as("SELECT MIN(played_at) FROM scrobbles WHERE user_id = ? AND played_at >= ? AND played_at < ?")
            .bind(user.id)
            .bind(from)
            .bind(to)
            .fetch_one(&st.db)
            .await?;
    let peak_hour =
        totals.by_hour.iter().enumerate().max_by_key(|(i, n)| (**n, std::cmp::Reverse(*i))).map(|(i, _)| i as i64);

    Ok(Some(json!({
        "schema": SCHEMA,
        "year": year,
        "user": user.display_name,
        "timezone": tz.name(),
        "totals": {
            "plays": totals.plays,
            "ms": totals.listening_ms,
            "ms_estimated": totals.plays_without_duration > 0,
            "artists": totals.artists,
            "albums": totals.albums,
            "tracks": totals.tracks,
            "days_with_music": days_with_music.0,
            "top_day": totals.top_day,
            "peak_hour": peak_hour,
            "first_play": first_play,
            "previous_plays": totals.previous_plays,
            "change_pct": totals.change_pct,
        },
        "top_artists": top_artists,
        "top_albums": top_albums,
        "top_tracks": top_tracks,
        "monthly": monthly,
        "evolution": evolution,
        "top_month": top_month,
        "soundtrack": soundtrack,
        "generated_at": now(),
    })))
}

/// Guarda una versión. Si ya existe (otra petición la ha generado a la vez), devuelve la existente.
async fn store(st: &AppState, user_id: i64, year: i32, version: i64, data: &Value) -> AppResult<WrappedRow> {
    let is_final = version > 0;
    let text = data.to_string();
    {
        let _turno = st.write_lock.lock().await;
        if is_final {
            sqlx::query(
                "INSERT INTO wrapped (user_id, year, version, is_final, generated_at, data) VALUES (?, ?, ?, 1, ?, ?) \
                 ON CONFLICT(user_id, year, version) DO NOTHING",
            )
        } else {
            sqlx::query(
                "INSERT INTO wrapped (user_id, year, version, is_final, generated_at, data) VALUES (?, ?, ?, 0, ?, ?) \
                 ON CONFLICT(user_id, year, version) DO UPDATE SET generated_at = excluded.generated_at, data = excluded.data",
            )
        }
        .bind(user_id)
        .bind(year)
        .bind(version)
        .bind(now())
        .bind(&text)
        .execute(&st.db)
        .await?;
    }
    let row = sqlx::query_as("SELECT id, year, version, is_final, generated_at, data FROM wrapped WHERE user_id = ? AND year = ? AND version = ?")
        .bind(user_id)
        .bind(year)
        .bind(version)
        .fetch_one(&st.db)
        .await?;
    Ok(row)
}

/// La versión más reciente de un año (la definitiva más alta; si no hay, la vista previa).
async fn latest(db: &Db, user_id: i64, year: i32) -> sqlx::Result<Option<WrappedRow>> {
    sqlx::query_as(
        "SELECT id, year, version, is_final, generated_at, data FROM wrapped WHERE user_id = ? AND year = ? \
         ORDER BY is_final DESC, version DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(year)
    .fetch_optional(db)
    .await
}

fn current_year(tz: &Tz) -> i32 {
    Utc::now().with_timezone(tz).year()
}

/// Devuelve el Wrapped de un año, generándolo si hace falta.
pub async fn get_or_build(st: &AppState, user: &User, year: i32) -> AppResult<WrappedRow> {
    let tz = user_tz(&user.timezone);
    let cy = current_year(&tz);
    if year > cy || year < 1970 {
        return Err(AppError::NotFound);
    }
    let existing = latest(&st.db, user.id, year).await?;
    if year == cy {
        if let Some(r) = existing
            && now() - r.generated_at < PREVIEW_TTL
        {
            return Ok(r);
        }
        let data = build(st, user, year).await?.ok_or(AppError::NotFound)?;
        return store(st, user.id, year, 0, &data).await;
    }
    match existing {
        Some(r) if r.is_final => Ok(r),
        _ => {
            let data = build(st, user, year).await?.ok_or(AppError::NotFound)?;
            store(st, user.id, year, 1, &data).await
        }
    }
}

/// Escuchas de un año en la BD ahora mismo (para saber si un Wrapped se ha quedado viejo).
async fn plays_in_year(db: &Db, user_id: i64, tz: &Tz, year: i32) -> sqlx::Result<i64> {
    let p = periods::year(*tz, year, Utc::now());
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM scrobbles WHERE user_id = ? AND played_at >= ? AND played_at < ?")
            .bind(user_id)
            .bind(p.from)
            .bind(p.to)
            .fetch_one(db)
            .await?;
    Ok(n)
}

fn data_plays(data: &str) -> i64 {
    serde_json::from_str::<Value>(data)
        .ok()
        .and_then(|v| v.pointer("/totals/plays").and_then(Value::as_i64))
        .unwrap_or(0)
}

async fn answers(db: &Db, wrapped_id: i64) -> sqlx::Result<Value> {
    let rows: Vec<(String, i64, bool)> =
        sqlx::query_as("SELECT question, chosen_rank, correct FROM wrapped_answers WHERE wrapped_id = ?")
            .bind(wrapped_id)
            .fetch_all(db)
            .await?;
    Ok(Value::Object(rows.into_iter().map(|(q, r, c)| (q, json!({"chosen_rank": r, "correct": c}))).collect()))
}

// ---------- API ----------

/// GET /api/v1/wrapped — años con escuchas y el Wrapped guardado de cada uno.
pub async fn list(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Value>> {
    let tz = user_tz(&u.timezone);
    let cy = current_year(&tz);
    let (first, last): (Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT MIN(played_at), MAX(played_at) FROM scrobbles WHERE user_id = ?")
            .bind(u.id)
            .fetch_one(&st.db)
            .await?;
    let mut years = vec![];
    if let (Some(f), Some(l)) = (first, last) {
        let fy = periods::local_date(&tz, f).year();
        let ly = periods::local_date(&tz, l).year().min(cy);
        for y in (fy..=ly).rev() {
            let plays = plays_in_year(&st.db, u.id, &tz, y).await?;
            if plays == 0 {
                continue;
            }
            let w = latest(&st.db, u.id, y).await?;
            years.push(json!({
                "year": y,
                "plays": plays,
                "current": y == cy,
                "wrapped": w.map(|w| {
                    let outdated = w.is_final && data_plays(&w.data) != plays;
                    json!({"id": w.id, "version": w.version, "is_final": w.is_final, "generated_at": w.generated_at, "outdated": outdated})
                }),
            }));
        }
    }
    Ok(Json(json!({"current_year": cy, "years": years})))
}

async fn full(st: &AppState, u: &User, w: WrappedRow) -> AppResult<Json<Value>> {
    let tz = user_tz(&u.timezone);
    let plays = plays_in_year(&st.db, u.id, &tz, w.year as i32).await?;
    let data: Value = serde_json::from_str(&w.data).map_err(anyhow::Error::from)?;
    let outdated = w.is_final && data.pointer("/totals/plays").and_then(Value::as_i64) != Some(plays);
    Ok(Json(json!({
        "id": w.id, "year": w.year, "version": w.version, "is_final": w.is_final, "generated_at": w.generated_at,
        "outdated": outdated,
        "answers": answers(&st.db, w.id).await?,
        "data": data,
    })))
}

/// GET /api/v1/wrapped/{year}
pub async fn get(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(year): Path<i32>,
) -> AppResult<Json<Value>> {
    let w = get_or_build(&st, &u, year).await?;
    full(&st, &u, w).await
}

/// POST /api/v1/wrapped/{year}/regenerate — rehace el Wrapped (nueva versión; las anteriores se conservan).
pub async fn regenerate(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(year): Path<i32>,
) -> AppResult<Json<Value>> {
    let tz = user_tz(&u.timezone);
    let cy = current_year(&tz);
    if year > cy {
        return Err(AppError::NotFound);
    }
    let data = build(&st, &u, year).await?.ok_or(AppError::NotFound)?;
    let version = if year == cy {
        0
    } else {
        let (max,): (Option<i64>,) = sqlx::query_as("SELECT MAX(version) FROM wrapped WHERE user_id = ? AND year = ?")
            .bind(u.id)
            .bind(year)
            .fetch_one(&st.db)
            .await?;
        max.unwrap_or(0).max(0) + 1
    };
    let w = store(&st, u.id, year, version, &data).await?;
    full(&st, &u, w).await
}

#[derive(Deserialize)]
pub struct AnswerBody {
    pub question: String,
    pub chosen_rank: i64,
}

/// POST /api/v1/wrapped/answers/{id} — guarda la respuesta a una prueba (solo cuenta la primera).
pub async fn answer(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(id): Path<i64>,
    Json(b): Json<AnswerBody>,
) -> AppResult<Json<Value>> {
    let key = match b.question.as_str() {
        "artist" => "/top_artists",
        "album" => "/top_albums",
        "track" => "/top_tracks",
        _ => return Err(AppError::BadRequest("pregunta desconocida (artist, album, track)".into())),
    };
    let row: Option<(String,)> = sqlx::query_as("SELECT data FROM wrapped WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(u.id)
        .fetch_optional(&st.db)
        .await?;
    let (data,) = row.ok_or(AppError::NotFound)?;
    let v: Value = serde_json::from_str(&data).map_err(anyhow::Error::from)?;
    let items = v.pointer(key).and_then(Value::as_array).cloned().unwrap_or_default();
    if !items.iter().any(|i| i["rank"].as_i64() == Some(b.chosen_rank)) {
        return Err(AppError::BadRequest("esa opción no está en la prueba".into()));
    }
    let correct = b.chosen_rank == 1;
    sqlx::query(
        "INSERT INTO wrapped_answers (wrapped_id, question, chosen_rank, correct, answered_at) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(wrapped_id, question) DO NOTHING",
    )
    .bind(id)
    .bind(&b.question)
    .bind(b.chosen_rank)
    .bind(correct)
    .bind(now())
    .execute(&st.db)
    .await?;
    Ok(Json(json!({"correct": correct, "answers": answers(&st.db, id).await?})))
}

/// GET /api/v1/wrapped/image/{hash} — imagen guardada para un Wrapped del usuario.
pub async fn image(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(hash): Path<String>,
) -> AppResult<Response> {
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::NotFound);
    }
    let used: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM wrapped WHERE user_id = ? AND instr(data, ?) > 0 LIMIT 1")
        .bind(u.id)
        .bind(format!("img:{hash}"))
        .fetch_optional(&st.db)
        .await?;
    if used.is_none() {
        return Err(AppError::NotFound);
    }
    media::from_disk(&st, &hash).await?.ok_or(AppError::NotFound)
}

// ---------- Tarea de fondo ----------

/// Año cuyo definitivo toca tener ya: el anterior, salvo el 1 de enero antes de las 06:00 (hora local),
/// para dar tiempo a que lleguen las escuchas que Navidrome tuviera guardadas.
fn final_due(tz: &Tz, now_utc: chrono::DateTime<Utc>) -> Option<i32> {
    let local = now_utc.with_timezone(tz);
    let early = local.month() == 1 && local.day() == 1 && local.hour() < FINAL_HOUR;
    (!early).then_some(local.year() - 1)
}

/// Cada hora: genera los Wrapped definitivos del año anterior que falten.
pub async fn run(st: AppState) {
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        if let Err(e) = generate_due(&st).await {
            tracing::warn!(error = %e, "no se han podido generar los Wrapped pendientes");
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

pub async fn generate_due(st: &AppState) -> AppResult<usize> {
    let users: Vec<User> = sqlx::query_as(
        "SELECT id, username, display_name, is_admin, is_approved, timezone, is_public, created_at FROM users WHERE is_approved = 1",
    )
    .fetch_all(&st.db)
    .await?;
    let mut n = 0;
    for u in users {
        let tz = user_tz(&u.timezone);
        let Some(year) = final_due(&tz, Utc::now()) else { continue };
        let has_final: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM wrapped WHERE user_id = ? AND year = ? AND is_final = 1 LIMIT 1")
                .bind(u.id)
                .bind(year)
                .fetch_optional(&st.db)
                .await?;
        if has_final.is_some() || plays_in_year(&st.db, u.id, &tz, year).await? == 0 {
            continue;
        }
        if let Some(data) = build(st, &u, year).await? {
            store(st, u.id, year, 1, &data).await?;
            tracing::info!(usuario = %u.username, año = year, "Wrapped definitivo generado");
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitivo_a_las_seis_del_uno_de_enero() {
        let tz: Tz = "Europe/Madrid".parse().unwrap();
        let at = |s: &str| chrono::DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        // 1 de enero 05:59 en Madrid = 04:59 UTC: todavía no toca el de 2026 (pero sí el de 2025).
        assert_eq!(final_due(&tz, at("2027-01-01T04:59:00Z")), None);
        assert_eq!(final_due(&tz, at("2027-01-01T05:00:00Z")), Some(2026));
        assert_eq!(final_due(&tz, at("2027-03-10T12:00:00Z")), Some(2026));
    }
}
