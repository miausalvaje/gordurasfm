//! Ingesta de escuchas: normalización, catálogo (artistas/álbumes/canciones), duplicados y cola de reenvío.

use serde_json::Value;
use sqlx::{Sqlite, Transaction};

use crate::{
    db::{Db, now},
    normalize,
};

/// Ventana para considerar duplicada una escucha (misma canción, mismo usuario).
pub const DEDUP_WINDOW_SECS: i64 = 60;
/// Las pistas más cortas no cuentan (regla de Last.fm), si se conoce la duración.
pub const MIN_DURATION_MS: i64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Navidrome,
    NavidromePoll,
    Lastfm,
    ImportFile,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Navidrome => "navidrome",
            Source::NavidromePoll => "navidrome_poll",
            Source::Lastfm => "lastfm",
            Source::ImportFile => "import_file",
        }
    }

    /// Solo las escuchas en vivo se reenvían (evita bucles y duplicados en los destinos).
    pub fn forwardable(self) -> bool {
        matches!(self, Source::Navidrome | Source::NavidromePoll)
    }
}

#[derive(Debug, Clone)]
pub struct Listen {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub duration_ms: Option<i64>,
    pub played_at: i64,
    pub recording_mbid: Option<String>,
    pub release_mbid: Option<String>,
    pub artist_mbid: Option<String>,
    pub client: Option<String>,
    pub source: Source,
    pub raw: Option<Value>,
    /// Portada externa (Last.fm) por si Navidrome no tiene el álbum.
    pub cover_url: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Stored(i64),
    Duplicate,
    Ignored(&'static str),
}

/// Guarda una escucha. Todo va en una transacción: escucha + filas de la cola de reenvío.
pub async fn ingest(db: &Db, user_id: i64, l: &Listen) -> sqlx::Result<Outcome> {
    let mut tx = db.begin().await?;
    let out = ingest_tx(&mut tx, user_id, l).await?;
    tx.commit().await?;
    Ok(out)
}

/// Guarda muchas escuchas en una sola transacción (importaciones): mucho más rápido.
pub async fn ingest_many(db: &Db, user_id: i64, listens: &[Listen]) -> sqlx::Result<Vec<Outcome>> {
    let mut tx = db.begin().await?;
    let mut out = Vec::with_capacity(listens.len());
    for l in listens {
        out.push(ingest_tx(&mut tx, user_id, l).await?);
    }
    tx.commit().await?;
    Ok(out)
}

async fn ingest_tx(tx: &mut Transaction<'_, Sqlite>, user_id: i64, l: &Listen) -> sqlx::Result<Outcome> {
    let artist = normalize::display(&l.artist);
    let track = normalize::display(&l.track);
    if artist.is_empty() || track.is_empty() {
        return Ok(Outcome::Ignored("faltan artista o título"));
    }
    if let Some(d) = l.duration_ms
        && d > 0
        && d < MIN_DURATION_MS
    {
        return Ok(Outcome::Ignored("pista de menos de 30 s"));
    }

    let artist_id = upsert_artist(tx, &artist, l.artist_mbid.as_deref()).await?;
    let track_id = upsert_track(tx, artist_id, &track, l.duration_ms, l.recording_mbid.as_deref()).await?;
    let album_id = match l.album.as_deref().map(normalize::display).filter(|a| !a.is_empty()) {
        Some(album) => {
            let aa = l.album_artist.as_deref().map(normalize::display).filter(|a| !a.is_empty());
            let album_artist_id = match aa {
                Some(name) if normalize::key(&name) != normalize::key(&artist) => {
                    upsert_artist(tx, &name, None).await?
                }
                _ => artist_id,
            };
            let cover = l.cover_url.as_deref().filter(|u| allowed_cover_url(u)).map(|u| format!("url:{u}"));
            Some(upsert_album(tx, album_artist_id, &album, l.release_mbid.as_deref(), cover.as_deref()).await?)
        }
        None => None,
    };

    let dup: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM scrobbles WHERE user_id = ? AND track_id = ? AND played_at BETWEEN ? AND ? LIMIT 1",
    )
    .bind(user_id)
    .bind(track_id)
    .bind(l.played_at - DEDUP_WINDOW_SECS)
    .bind(l.played_at + DEDUP_WINDOW_SECS)
    .fetch_optional(&mut **tx)
    .await?;
    if dup.is_some() {
        return Ok(Outcome::Duplicate);
    }

    let t = now();
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO scrobbles (user_id, track_id, album_id, artist_id, played_at, duration_ms, source, client, raw, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(track_id)
    .bind(album_id)
    .bind(artist_id)
    .bind(l.played_at)
    .bind(l.duration_ms.filter(|d| *d > 0))
    .bind(l.source.as_str())
    .bind(&l.client)
    .bind(l.raw.as_ref().map(|v| v.to_string()))
    .bind(t)
    .fetch_one(&mut **tx)
    .await?;

    if l.source.forwardable() {
        // Un destino activo con la conexión rota se encola en pausa: se reanuda al reconectar.
        sqlx::query(
            "INSERT INTO forward_queue (scrobble_id, user_id, target, status, next_attempt_at, updated_at) \
             SELECT ?, ?, provider, CASE status WHEN 'ok' THEN 'pending' ELSE 'paused' END, ?, ? \
             FROM connections WHERE user_id = ? AND forward_enabled = 1",
        )
        .bind(id)
        .bind(user_id)
        .bind(t)
        .bind(t)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(Outcome::Stored(id))
}

async fn upsert_artist(tx: &mut Transaction<'_, Sqlite>, name: &str, mbid: Option<&str>) -> sqlx::Result<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO artists (name, name_key, mbid) VALUES (?, ?, ?) \
         ON CONFLICT(name_key) DO UPDATE SET mbid = COALESCE(artists.mbid, excluded.mbid) RETURNING id",
    )
    .bind(name)
    .bind(normalize::key(name))
    .bind(mbid.filter(|m| !m.is_empty()))
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

async fn upsert_track(
    tx: &mut Transaction<'_, Sqlite>,
    artist_id: i64,
    title: &str,
    duration_ms: Option<i64>,
    mbid: Option<&str>,
) -> sqlx::Result<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO tracks (title, title_key, artist_id, duration_ms, mbid) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(artist_id, title_key) DO UPDATE SET \
           duration_ms = COALESCE(excluded.duration_ms, tracks.duration_ms), \
           mbid = COALESCE(tracks.mbid, excluded.mbid) RETURNING id",
    )
    .bind(title)
    .bind(normalize::key(title))
    .bind(artist_id)
    .bind(duration_ms.filter(|d| *d > 0))
    .bind(mbid.filter(|m| !m.is_empty()))
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// Solo portadas https de los servidores de imágenes de Last.fm.
pub fn allowed_cover_url(u: &str) -> bool {
    let Some(rest) = u.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', ':']).next().unwrap_or("");
    crate::lastfm::IMAGE_HOSTS.contains(&host)
}

async fn upsert_album(
    tx: &mut Transaction<'_, Sqlite>,
    artist_id: i64,
    title: &str,
    mbid: Option<&str>,
    cover_ref: Option<&str>,
) -> sqlx::Result<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO albums (title, title_key, artist_id, mbid, cover_ref) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(artist_id, title_key) DO UPDATE SET mbid = COALESCE(albums.mbid, excluded.mbid), \
           cover_ref = COALESCE(albums.cover_ref, excluded.cover_ref) RETURNING id",
    )
    .bind(title)
    .bind(normalize::key(title))
    .bind(artist_id)
    .bind(mbid.filter(|m| !m.is_empty()))
    .bind(cover_ref)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_portadas_de_last_fm() {
        assert!(allowed_cover_url("https://lastfm.freetls.fastly.net/i/u/300x300/x.jpg"));
        assert!(allowed_cover_url("https://lastfm-img.freetls.fastly.net/i/u/300x300/x.jpg"));
        assert!(!allowed_cover_url("http://lastfm.freetls.fastly.net/i/u/x.jpg"));
        assert!(!allowed_cover_url("https://lastfm.freetls.fastly.net.evil.com/x.jpg"));
        assert!(!allowed_cover_url("https://192.168.1.1/x.jpg"));
    }

    fn listen(track: &str, at: i64) -> Listen {
        Listen {
            artist: "Rosalía".into(),
            track: track.into(),
            album: Some("Motomami".into()),
            album_artist: None,
            duration_ms: Some(180_000),
            played_at: at,
            recording_mbid: None,
            release_mbid: None,
            artist_mbid: None,
            client: None,
            source: Source::Navidrome,
            raw: None,
            cover_url: None,
        }
    }

    async fn user(db: &Db) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "INSERT INTO users (username, display_name, created_at) VALUES ('ana', 'ana', 0) RETURNING id",
        )
        .fetch_one(db)
        .await
        .unwrap()
        .0
    }

    #[tokio::test]
    async fn duplicado_dentro_de_60s() {
        let db = crate::db::open_memory().await.unwrap();
        let u = user(&db).await;
        assert!(matches!(ingest(&db, u, &listen("Saoko", 1000)).await.unwrap(), Outcome::Stored(_)));
        assert_eq!(ingest(&db, u, &listen("Saoko", 1060)).await.unwrap(), Outcome::Duplicate);
        assert_eq!(ingest(&db, u, &listen("  SAOKO ", 940)).await.unwrap(), Outcome::Duplicate);
        assert!(matches!(ingest(&db, u, &listen("Saoko", 1061)).await.unwrap(), Outcome::Stored(_)));
        assert!(matches!(ingest(&db, u, &listen("Candy", 1000)).await.unwrap(), Outcome::Stored(_)));
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scrobbles").fetch_one(&db).await.unwrap();
        assert_eq!(n, 3);
        let (t,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks").fetch_one(&db).await.unwrap();
        assert_eq!(t, 2, "«Saoko» y «  SAOKO » son la misma canción");
    }

    #[tokio::test]
    async fn pistas_cortas_y_vacias() {
        let db = crate::db::open_memory().await.unwrap();
        let u = user(&db).await;
        let mut l = listen("Intro", 1);
        l.duration_ms = Some(29_000);
        assert!(matches!(ingest(&db, u, &l).await.unwrap(), Outcome::Ignored(_)));
        l.duration_ms = None; // duración desconocida: cuenta
        assert!(matches!(ingest(&db, u, &l).await.unwrap(), Outcome::Stored(_)));
        l.track = "   ".into();
        assert!(matches!(ingest(&db, u, &l).await.unwrap(), Outcome::Ignored(_)));
    }

    #[tokio::test]
    async fn encola_reenvio_solo_en_vivo() {
        let db = crate::db::open_memory().await.unwrap();
        let u = user(&db).await;
        sqlx::query(
            "INSERT INTO connections (user_id, provider, secret_enc, updated_at) VALUES (?, 'listenbrainz', x'00', 0)",
        )
        .bind(u)
        .execute(&db)
        .await
        .unwrap();
        ingest(&db, u, &listen("A", 100)).await.unwrap();
        let mut imp = listen("B", 100);
        imp.source = Source::Lastfm;
        ingest(&db, u, &imp).await.unwrap();
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT target, status FROM forward_queue").fetch_all(&db).await.unwrap();
        assert_eq!(rows, vec![("listenbrainz".to_string(), "pending".to_string())]);
    }
}
