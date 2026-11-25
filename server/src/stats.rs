//! Estadísticas: gráfica por cubos, totales y tops (álbumes, canciones, artistas) de un periodo.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Datelike, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;

use crate::{
    auth::CurrentUser,
    db::Db,
    error::{AppError, AppResult},
    periods::{self, Period, PeriodKey},
    state::AppState,
};

const CACHE_TTL: Duration = Duration::from_secs(600);
const CACHE_MAX: usize = 1000;

/// Caché de respuestas. La clave incluye la «versión» de los datos del usuario
/// (número de escuchas y última id), así que una escucha nueva invalida lo anterior.
#[derive(Default)]
pub struct StatsCache(Mutex<HashMap<String, (Instant, Value)>>);

impl StatsCache {
    fn get(&self, k: &str) -> Option<Value> {
        let m = self.0.lock().unwrap();
        m.get(k).filter(|(t, _)| t.elapsed() < CACHE_TTL).map(|(_, v)| v.clone())
    }

    fn put(&self, k: String, v: Value) {
        let mut m = self.0.lock().unwrap();
        if m.len() >= CACHE_MAX {
            m.retain(|_, (t, _)| t.elapsed() < CACHE_TTL);
            if m.len() >= CACHE_MAX {
                m.clear();
            }
        }
        m.insert(k, (Instant::now(), v));
    }
}

async fn data_version(db: &Db, user_id: i64) -> sqlx::Result<(i64, i64)> {
    sqlx::query_as("SELECT COUNT(*), COALESCE(MAX(id), 0) FROM scrobbles WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(db)
        .await
}

fn user_tz(tz: &str) -> Tz {
    tz.parse().unwrap_or(chrono_tz::Europe::Madrid)
}

fn parse_period(p: Option<&str>) -> AppResult<PeriodKey> {
    let p = p.unwrap_or("this_week");
    PeriodKey::parse(p).ok_or_else(|| {
        AppError::BadRequest(format!(
            "periodo desconocido: {p} (this_week, last_week, this_month, last_month, this_year, last_year, all_time)"
        ))
    })
}

pub async fn resolve_period(db: &Db, user_id: i64, tz: Tz, key: PeriodKey, now: DateTime<Utc>) -> sqlx::Result<Period> {
    let first: Option<i64> = if key == PeriodKey::AllTime {
        sqlx::query_as::<_, (Option<i64>,)>("SELECT MIN(played_at) FROM scrobbles WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(db)
            .await?
            .0
    } else {
        None
    };
    Ok(periods::resolve(key, tz, now, first))
}

#[derive(Debug, Serialize)]
pub struct ChartPoint {
    pub label: String,
    pub start: i64,
    pub end: i64,
    pub plays: i64,
}

#[derive(Debug, Serialize)]
pub struct Totals {
    pub plays: i64,
    /// Tiempo escuchado. Las escuchas sin duración conocida no suman (ver `plays_without_duration`).
    pub listening_ms: i64,
    pub plays_without_duration: i64,
    pub artists: i64,
    pub albums: i64,
    pub tracks: i64,
    pub avg_plays_per_day: f64,
    pub top_day: Option<TopDay>,
    pub previous_plays: Option<i64>,
    /// Variación frente al periodo anterior, en %. `None` si el anterior no tiene escuchas.
    pub change_pct: Option<f64>,
    /// Escuchas por hora local (0-23) y por día de la semana (0 = lunes).
    pub by_hour: Vec<i64>,
    pub by_weekday: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct TopDay {
    pub date: String,
    pub plays: i64,
}

struct TotalsRow {
    plays: i64,
    ms: Option<i64>,
    no_dur: Option<i64>,
    artists: i64,
    albums: i64,
    tracks: i64,
}

/// Gráfica y totales del periodo.
pub async fn chart_and_totals(
    db: &Db,
    user_id: i64,
    p: &Period,
    tz: Tz,
    now: i64,
) -> sqlx::Result<(Vec<ChartPoint>, Totals)> {
    // Se agrupa en SQL por tramos de 1 h (o de 15 min si la zona tiene desfases de media hora):
    // los límites de los cubos locales caen siempre en un múltiplo del tramo.
    let step = bucket_step(&tz, p.from, p.to);
    let slots: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT played_at / ? AS b, COUNT(*) FROM scrobbles WHERE user_id = ? AND played_at >= ? AND played_at < ? \
         GROUP BY b ORDER BY b",
    )
    .bind(step)
    .bind(user_id)
    .bind(p.from)
    .bind(p.to)
    .fetch_all(db)
    .await?;

    let mut chart: Vec<ChartPoint> =
        p.buckets.iter().map(|b| ChartPoint { label: b.label.clone(), start: b.start, end: b.end, plays: 0 }).collect();
    let mut by_hour = vec![0i64; 24];
    let mut by_weekday = vec![0i64; 7];
    let mut days: HashMap<chrono::NaiveDate, i64> = HashMap::new();
    let mut bi = 0;
    for (slot, n) in &slots {
        let t = slot * step;
        while bi < chart.len() && t >= chart[bi].end {
            bi += 1;
        }
        if bi < chart.len() && t >= chart[bi].start {
            chart[bi].plays += n;
        }
        let local = DateTime::from_timestamp(t, 0).unwrap_or_default().with_timezone(&tz);
        by_hour[local.hour() as usize] += n;
        by_weekday[local.weekday().num_days_from_monday() as usize] += n;
        *days.entry(local.date_naive()).or_default() += n;
    }
    let top_day = days
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(d, n)| TopDay { date: d.format("%Y-%m-%d").to_string(), plays: n });

    // Totales. El índice (usuario, tiempo, canción, álbum, artista, duración) cubre todas estas consultas.
    let (plays, ms_own, null_dur): (i64, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT COUNT(*), SUM(duration_ms), SUM(duration_ms IS NULL) FROM scrobbles \
         WHERE user_id = ? AND played_at >= ? AND played_at < ?",
    )
    .bind(user_id)
    .bind(p.from)
    .bind(p.to)
    .fetch_one(db)
    .await?;
    // Las escuchas sin duración propia (p. ej. de Last.fm) usan la de la canción, si se conoce.
    let (ms_track, no_dur): (Option<i64>, Option<i64>) = if null_dur.unwrap_or(0) > 0 {
        sqlx::query_as(
            "SELECT SUM(x.c * t.duration_ms), SUM(CASE WHEN t.duration_ms IS NULL THEN x.c ELSE 0 END) \
             FROM (SELECT track_id, COUNT(*) AS c FROM scrobbles WHERE user_id = ? AND played_at >= ? AND played_at < ? \
                   AND duration_ms IS NULL GROUP BY track_id) x JOIN tracks t ON t.id = x.track_id",
        )
        .bind(user_id)
        .bind(p.from)
        .bind(p.to)
        .fetch_one(db)
        .await?
    } else {
        (None, Some(0))
    };
    let mut distinct = [0i64; 3];
    for (i, col) in ["artist_id", "album_id", "track_id"].iter().enumerate() {
        let sql = format!(
            "SELECT COUNT(*) FROM (SELECT 1 FROM scrobbles WHERE user_id = ? AND played_at >= ? AND played_at < ? \
             AND {col} IS NOT NULL GROUP BY {col})"
        );
        distinct[i] = sqlx::query_as::<_, (i64,)>(sqlx::AssertSqlSafe(sql))
            .bind(user_id)
            .bind(p.from)
            .bind(p.to)
            .fetch_one(db)
            .await?
            .0;
    }
    let r = TotalsRow {
        plays,
        ms: Some(ms_own.unwrap_or(0) + ms_track.unwrap_or(0)),
        no_dur,
        artists: distinct[0],
        albums: distinct[1],
        tracks: distinct[2],
    };

    let previous_plays = match p.previous {
        Some((pf, pt)) => Some(
            sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*) FROM scrobbles WHERE user_id = ? AND played_at >= ? AND played_at < ?",
            )
            .bind(user_id)
            .bind(pf)
            .bind(pt)
            .fetch_one(db)
            .await?
            .0,
        ),
        None => None,
    };
    let change_pct =
        previous_plays.filter(|p| *p > 0).map(|prev| ((r.plays - prev) as f64 / prev as f64 * 1000.0).round() / 10.0);

    // Media por día sobre los días transcurridos del periodo.
    let elapsed_end = if p.key.is_current() { now.min(p.to) } else { p.to };
    let days_n = ((elapsed_end - p.from) as f64 / 86400.0).ceil().max(1.0);
    let avg = (r.plays as f64 / days_n * 10.0).round() / 10.0;

    Ok((
        chart,
        Totals {
            plays: r.plays,
            listening_ms: r.ms.unwrap_or(0),
            plays_without_duration: r.no_dur.unwrap_or(0),
            artists: r.artists,
            albums: r.albums,
            tracks: r.tracks,
            avg_plays_per_day: avg,
            top_day,
            previous_plays,
            change_pct,
            by_hour,
            by_weekday,
        },
    ))
}

/// Tramo de agrupación: 1 h si la zona horaria solo tiene desfases de horas enteras en el periodo, si no 15 min.
fn bucket_step(tz: &Tz, from: i64, to: i64) -> i64 {
    use chrono::{Offset, TimeZone};
    let whole = [from, to - 1, (from + to) / 2].iter().all(|t| {
        let dt = DateTime::from_timestamp(*t, 0).unwrap_or_default();
        tz.offset_from_utc_datetime(&dt.naive_utc()).fix().local_minus_utc() % 3600 == 0
    });
    if whole { 3600 } else { 900 }
}

fn cover_url(album_id: Option<i64>, cover_ref: &Option<String>) -> Option<String> {
    match (album_id, cover_ref) {
        (Some(id), Some(_)) => Some(format!("/api/v1/media/cover/{id}")),
        _ => None,
    }
}

/// Posiciones con empates compartidos (1, 2, 2, 4). `start` = posición del primero de la página.
fn competition_ranks(plays: &[i64], start: i64, offset: i64) -> Vec<i64> {
    let mut out = Vec::with_capacity(plays.len());
    for (i, p) in plays.iter().enumerate() {
        if i > 0 && *p == plays[i - 1] {
            out.push(out[i - 1]);
        } else if i == 0 {
            out.push(start);
        } else {
            out.push(offset + i as i64 + 1);
        }
    }
    out
}

/// Columna de tiempo para filtrar: en rangos muy largos se usa `+played_at`, que impide a SQLite usar el
/// índice por tiempo y le hace usar el agrupado (usuario, canción/álbum/artista), mucho más rápido ahí.
fn range_col(from: i64, to: i64) -> &'static str {
    if to - from > 2 * 366 * 86400 { "+played_at" } else { "played_at" }
}

/// Posición del primer elemento de la página: 1 + cuántos tienen estrictamente más escuchas.
async fn first_rank(db: &Db, group_col: &str, user_id: i64, from: i64, to: i64, plays: i64) -> sqlx::Result<i64> {
    let col = match group_col {
        "album_id" => "album_id",
        "artist_id" => "artist_id",
        _ => "track_id",
    };
    let pa = range_col(from, to);
    let sql = format!(
        "SELECT COUNT(*) FROM (SELECT COUNT(*) AS c FROM scrobbles WHERE user_id = ? AND {pa} >= ? AND {pa} < ? \
         AND {col} IS NOT NULL GROUP BY {col} HAVING c > ?)"
    );
    let (n,): (i64,) =
        sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(user_id).bind(from).bind(to).bind(plays).fetch_one(db).await?;
    Ok(n + 1)
}

#[derive(Debug, Serialize)]
pub struct TopAlbum {
    pub rank: i64,
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub plays: i64,
    pub cover: Option<String>,
    pub last_played: i64,
}

#[derive(FromRow)]
struct AlbumRow {
    id: i64,
    title: String,
    artist: String,
    plays: i64,
    last: i64,
    cover_ref: Option<String>,
}

pub async fn top_albums(
    db: &Db,
    user_id: i64,
    from: i64,
    to: i64,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<TopAlbum>> {
    let pa = range_col(from, to);
    let rows: Vec<AlbumRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "WITH t AS (SELECT album_id, COUNT(*) AS plays, MAX(played_at) AS last FROM scrobbles \
                    WHERE user_id = ? AND {pa} >= ? AND {pa} < ? AND album_id IS NOT NULL \
                    GROUP BY album_id ORDER BY plays DESC, last DESC LIMIT ? OFFSET ?) \
         SELECT al.id, al.title, ar.name AS artist, t.plays, t.last, al.cover_ref \
         FROM t JOIN albums al ON al.id = t.album_id JOIN artists ar ON ar.id = al.artist_id \
         ORDER BY t.plays DESC, t.last DESC",
    )))
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    let start = match rows.first() {
        Some(r) if offset > 0 => first_rank(db, "album_id", user_id, from, to, r.plays).await?,
        _ => 1,
    };
    let ranks = competition_ranks(&rows.iter().map(|r| r.plays).collect::<Vec<_>>(), start, offset);
    Ok(rows
        .into_iter()
        .zip(ranks)
        .map(|(r, rank)| TopAlbum {
            rank,
            id: r.id,
            cover: cover_url(Some(r.id), &r.cover_ref),
            title: r.title,
            artist: r.artist,
            plays: r.plays,
            last_played: r.last,
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct TopTrack {
    pub rank: i64,
    pub id: i64,
    pub title: String,
    pub artist: String,
    /// El álbum con el que más se ha escuchado en el periodo.
    pub album: Option<String>,
    pub album_id: Option<i64>,
    pub plays: i64,
    pub cover: Option<String>,
    pub duration_ms: Option<i64>,
    pub last_played: i64,
}

#[derive(FromRow)]
struct TrackRow {
    id: i64,
    title: String,
    artist: String,
    plays: i64,
    last: i64,
    duration_ms: Option<i64>,
    album_id: Option<i64>,
    album: Option<String>,
    cover_ref: Option<String>,
}

pub async fn top_tracks(
    db: &Db,
    user_id: i64,
    from: i64,
    to: i64,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<TopTrack>> {
    let pa = range_col(from, to);
    let rows: Vec<TrackRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "WITH t AS (SELECT track_id, COUNT(*) AS plays, MAX(played_at) AS last FROM scrobbles \
                    WHERE user_id = ? AND {pa} >= ? AND {pa} < ? \
                    GROUP BY track_id ORDER BY plays DESC, last DESC LIMIT ? OFFSET ?), \
              x AS (SELECT t.track_id, t.plays, t.last, \
                           (SELECT s.album_id FROM scrobbles s WHERE s.user_id = ? AND s.track_id = t.track_id \
                              AND s.played_at >= ? AND s.played_at < ? AND s.album_id IS NOT NULL \
                            GROUP BY s.album_id ORDER BY COUNT(*) DESC, MAX(s.played_at) DESC LIMIT 1) AS album_id FROM t) \
         SELECT tr.id, tr.title, ar.name AS artist, x.plays, x.last, tr.duration_ms, \
                al.id AS album_id, al.title AS album, al.cover_ref \
         FROM x JOIN tracks tr ON tr.id = x.track_id JOIN artists ar ON ar.id = tr.artist_id \
         LEFT JOIN albums al ON al.id = x.album_id \
         ORDER BY x.plays DESC, x.last DESC",
    )))
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .bind(offset)
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?;
    let start = match rows.first() {
        Some(r) if offset > 0 => first_rank(db, "track_id", user_id, from, to, r.plays).await?,
        _ => 1,
    };
    let ranks = competition_ranks(&rows.iter().map(|r| r.plays).collect::<Vec<_>>(), start, offset);
    Ok(rows
        .into_iter()
        .zip(ranks)
        .map(|(r, rank)| TopTrack {
            rank,
            id: r.id,
            cover: cover_url(r.album_id, &r.cover_ref),
            title: r.title,
            artist: r.artist,
            album: r.album,
            album_id: r.album_id,
            plays: r.plays,
            duration_ms: r.duration_ms,
            last_played: r.last,
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct TopArtist {
    pub rank: i64,
    pub id: i64,
    pub name: String,
    pub plays: i64,
    /// Imagen del artista (si Navidrome la tiene).
    pub image: Option<String>,
    /// Hasta 4 portadas de sus álbumes más escuchados, para un collage si no hay imagen.
    pub covers: Vec<String>,
    pub last_played: i64,
}

#[derive(FromRow)]
struct ArtistRow {
    id: i64,
    name: String,
    plays: i64,
    last: i64,
    image_ref: Option<String>,
    album_ids: Option<String>,
}

pub async fn top_artists(
    db: &Db,
    user_id: i64,
    from: i64,
    to: i64,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<TopArtist>> {
    let pa = range_col(from, to);
    let rows: Vec<ArtistRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "WITH t AS (SELECT artist_id, COUNT(*) AS plays, MAX(played_at) AS last FROM scrobbles \
                    WHERE user_id = ? AND {pa} >= ? AND {pa} < ? \
                    GROUP BY artist_id ORDER BY plays DESC, last DESC LIMIT ? OFFSET ?), \
              g AS (SELECT t.artist_id, \
                           (SELECT GROUP_CONCAT(album_id) FROM (SELECT s.album_id FROM scrobbles s JOIN albums al ON al.id = s.album_id \
                              WHERE s.user_id = ? AND s.artist_id = t.artist_id AND s.played_at >= ? AND s.played_at < ? \
                                AND al.cover_ref IS NOT NULL GROUP BY s.album_id ORDER BY COUNT(*) DESC LIMIT 4)) AS album_ids FROM t) \
         SELECT ar.id, ar.name, t.plays, t.last, ar.image_ref, g.album_ids \
         FROM t JOIN artists ar ON ar.id = t.artist_id LEFT JOIN g ON g.artist_id = t.artist_id \
         ORDER BY t.plays DESC, t.last DESC",
    )))
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .bind(offset)
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?;
    let start = match rows.first() {
        Some(r) if offset > 0 => first_rank(db, "artist_id", user_id, from, to, r.plays).await?,
        _ => 1,
    };
    let ranks = competition_ranks(&rows.iter().map(|r| r.plays).collect::<Vec<_>>(), start, offset);
    Ok(rows
        .into_iter()
        .zip(ranks)
        .map(|(r, rank)| TopArtist {
            rank,
            id: r.id,
            image: r.image_ref.as_ref().map(|_| format!("/api/v1/media/artist/{}", r.id)),
            covers: r
                .album_ids
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|id| format!("/api/v1/media/cover/{id}"))
                .collect(),
            name: r.name,
            plays: r.plays,
            last_played: r.last,
        })
        .collect())
}

#[derive(Deserialize)]
pub struct StatsQuery {
    pub period: Option<String>,
    pub limit: Option<i64>,
}

/// GET /api/v1/stats — todo lo que necesita la portada: gráfica, totales y los tres tops.
pub async fn get_stats(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Query<StatsQuery>,
) -> AppResult<Json<Value>> {
    let key = parse_period(q.period.as_deref())?;
    let limit = q.limit.unwrap_or(10).clamp(1, 50);
    let tz = user_tz(&u.timezone);
    let now = Utc::now();
    let p = resolve_period(&st.db, u.id, tz, key, now).await?;
    let (n, maxid) = data_version(&st.db, u.id).await?;
    let ck = format!("stats:{}:{}:{}:{}:{}:{}:{}", u.id, key.as_str(), u.timezone, p.from, n, maxid, limit);
    if let Some(v) = st.stats_cache.get(&ck) {
        return Ok(Json(v));
    }
    let (chart, totals) = chart_and_totals(&st.db, u.id, &p, tz, now.timestamp()).await?;
    let albums = top_albums(&st.db, u.id, p.from, p.to, limit, 0).await?;
    let tracks = top_tracks(&st.db, u.id, p.from, p.to, limit, 0).await?;
    let artists = top_artists(&st.db, u.id, p.from, p.to, limit, 0).await?;
    let v = json!({
        "period": {"key": key, "timezone": p.timezone, "from": p.from, "to": p.to, "unit": p.unit, "previous": p.previous},
        "chart": chart,
        "totals": totals,
        "top_albums": albums,
        "top_tracks": tracks,
        "top_artists": artists,
    });
    st.stats_cache.put(ck, v.clone());
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct TopQuery {
    pub period: Option<String>,
    pub limit: Option<i64>,
    pub page: Option<i64>,
}

/// GET /api/v1/top/{albums|tracks|artists}?period=&page=&limit=
pub async fn get_top(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(kind): Path<String>,
    Query(q): Query<TopQuery>,
) -> AppResult<Json<Value>> {
    let key = parse_period(q.period.as_deref())?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * limit;
    let tz = user_tz(&u.timezone);
    let p = resolve_period(&st.db, u.id, tz, key, Utc::now()).await?;
    let (n, maxid) = data_version(&st.db, u.id).await?;
    let ck =
        format!("top:{kind}:{}:{}:{}:{}:{}:{}:{}:{}", u.id, key.as_str(), u.timezone, p.from, n, maxid, limit, page);
    if let Some(v) = st.stats_cache.get(&ck) {
        return Ok(Json(v));
    }
    let (items, col) = match kind.as_str() {
        "albums" => (serde_json::to_value(top_albums(&st.db, u.id, p.from, p.to, limit, offset).await?)?, "album_id"),
        "tracks" => (serde_json::to_value(top_tracks(&st.db, u.id, p.from, p.to, limit, offset).await?)?, "track_id"),
        "artists" => {
            (serde_json::to_value(top_artists(&st.db, u.id, p.from, p.to, limit, offset).await?)?, "artist_id")
        }
        _ => return Err(AppError::NotFound),
    };
    let sql = format!(
        "SELECT COUNT(DISTINCT {col}) FROM scrobbles WHERE user_id = ? AND {pa} >= ? AND {pa} < ? AND {col} IS NOT NULL",
        pa = range_col(p.from, p.to)
    );
    let (total,): (i64,) =
        sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(u.id).bind(p.from).bind(p.to).fetch_one(&st.db).await?;
    let v = json!({"period": {"key": key, "from": p.from, "to": p.to}, "page": page, "limit": limit, "total": total, "items": items});
    st.stats_cache.put(ck, v.clone());
    Ok(Json(v))
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Other(e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tramo_segun_la_zona() {
        let madrid: Tz = "Europe/Madrid".parse().unwrap();
        let india: Tz = "Asia/Kolkata".parse().unwrap();
        assert_eq!(bucket_step(&madrid, 1_700_000_000, 1_730_000_000), 3600);
        assert_eq!(bucket_step(&india, 1_700_000_000, 1_730_000_000), 900);
    }

    #[test]
    fn posiciones_con_empates() {
        assert_eq!(competition_ranks(&[10, 8, 8, 5, 5, 5, 1], 1, 0), vec![1, 2, 2, 4, 4, 4, 7]);
        // Segunda página que empieza empatada con la anterior (posición 9 calculada aparte)
        assert_eq!(competition_ranks(&[3, 3, 2], 9, 10), vec![9, 9, 13]);
    }
}
