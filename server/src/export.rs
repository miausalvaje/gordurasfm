//! Exportación del historial del usuario (JSON o CSV), en un formato que se puede volver a importar.

use axum::{
    body::Body,
    extract::{Query, State},
    http::header,
    response::Response,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{auth::CurrentUser, error::AppResult, import_file::split_csv_line, state::AppState};

#[derive(Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>,
}

#[derive(Serialize, FromRow)]
pub struct Row {
    pub played_at: i64,
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub duration_ms: Option<i64>,
    pub source: String,
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// GET /api/v1/export?format=json|csv
pub async fn export(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Query<ExportQuery>,
) -> AppResult<Response> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.played_at, a.name AS artist, t.title AS track, al.title AS album, aa.name AS album_artist, \
                COALESCE(s.duration_ms, t.duration_ms) AS duration_ms, s.source \
         FROM scrobbles s JOIN artists a ON a.id = s.artist_id JOIN tracks t ON t.id = s.track_id \
         LEFT JOIN albums al ON al.id = s.album_id LEFT JOIN artists aa ON aa.id = al.artist_id \
         WHERE s.user_id = ? ORDER BY s.played_at",
    )
    .bind(u.id)
    .fetch_all(&st.db)
    .await?;
    let date = chrono::Utc::now().format("%Y-%m-%d");
    let (body, mime, ext) = if q.format.as_deref() == Some("csv") {
        let mut out = String::from("played_at,artist,album,track,duration_ms,source\n");
        for r in &rows {
            out.push_str(&format!(
                "{},{},{},{},{},{}\n",
                r.played_at,
                csv_field(&r.artist),
                csv_field(r.album.as_deref().unwrap_or("")),
                csv_field(&r.track),
                r.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
                r.source
            ));
        }
        debug_assert!(rows.is_empty() || split_csv_line(out.lines().nth(1).unwrap_or("")).len() == 6);
        (out, "text/csv; charset=utf-8", "csv")
    } else {
        (serde_json::to_string(&rows).map_err(anyhow::Error::from)?, "application/json", "json")
    };
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"gordurasfm-{}-{date}.{ext}\"", u.username.replace('"', "")),
        )
        .body(Body::from(body))
        .unwrap())
}
