//! Endpoint compatible con ListenBrainz (`/1/...`) para que Navidrome nos mande las escuchas.
//!
//! - `GET  /1/validate-token`
//! - `POST /1/submit-listens` (`single`, `import`, `playing_now`)
//! - `GET  /1/<resto>` → paso transparente a la API real (Navidrome pide ahí metadatos de artistas)
//!
//! Tokens: primero los de OwnFM; si no, el token real de ListenBrainz que un usuario haya
//! conectado (por HMAC); si no es de nadie, se pasa tal cual a ListenBrainz (configurable).

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    crypto,
    db::now,
    scrobble::{self, Listen, Outcome, Source},
    state::{AppState, NowPlaying},
};

/// Las escuchas anteriores a esto se rechazan (mismo límite que ListenBrainz, octubre de 2002).
const MIN_LISTENED_AT: i64 = 1_033_430_400;
const MAX_IMPORT: usize = 1000;
const MAX_BODY: usize = 12 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/1/validate-token", get(validate_token))
        .route("/1/submit-listens", post(submit_listens))
        .route("/1/{*rest}", get(passthrough_get))
        .layer(DefaultBodyLimit::max(MAX_BODY))
}

fn lb_error(status: StatusCode, msg: &str) -> Response {
    (status, axum::Json(json!({"code": status.as_u16(), "error": msg}))).into_response()
}

fn token_from(headers: &HeaderMap) -> Option<String> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, tok) = v.trim().split_once(' ')?;
    (scheme.eq_ignore_ascii_case("token") || scheme.eq_ignore_ascii_case("bearer"))
        .then(|| tok.trim().to_string())
        .filter(|t| !t.is_empty())
}

#[derive(Debug, Clone)]
pub struct TokenOwner {
    pub user_id: i64,
    pub username: String,
}

/// Busca de quién es un token: de OwnFM o un token de ListenBrainz conectado.
pub async fn resolve_token(st: &AppState, token: &str) -> sqlx::Result<Option<TokenOwner>> {
    let h = crypto::token_hash(token);
    let own: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.username FROM api_tokens t JOIN users u ON u.id = t.user_id \
         WHERE t.token_hash = ? AND t.kind = 'scrobble' AND u.is_approved = 1",
    )
    .bind(&h)
    .fetch_optional(&st.db)
    .await?;
    if let Some((user_id, username)) = own {
        sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE token_hash = ?")
            .bind(now())
            .bind(&h)
            .execute(&st.db)
            .await?;
        return Ok(Some(TokenOwner { user_id, username }));
    }
    let lb: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.username FROM connections c JOIN users u ON u.id = c.user_id \
         WHERE c.provider = 'listenbrainz' AND c.lookup_hmac = ? AND u.is_approved = 1",
    )
    .bind(st.crypto.lookup_hmac(token))
    .fetch_optional(&st.db)
    .await?;
    Ok(lb.map(|(user_id, username)| TokenOwner { user_id, username }))
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

async fn validate_token(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<TokenQuery>) -> Response {
    let Some(token) = token_from(&headers).or(q.token.filter(|t| !t.is_empty())) else {
        return lb_error(StatusCode::BAD_REQUEST, "You need to provide an Authorization token.");
    };
    match resolve_token(&st, &token).await {
        Ok(Some(o)) => {
            axum::Json(json!({"code": 200, "message": "Token valid.", "valid": true, "user_name": o.username}))
                .into_response()
        }
        Ok(None) if st.cfg.listenbrainz.passthrough_unknown_tokens => {
            forward_raw(&st, reqwest::Method::GET, "validate-token", "", Some(&token), None).await
        }
        Ok(None) => axum::Json(json!({"code": 200, "message": "Token invalid.", "valid": false})).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "validate-token");
            lb_error(StatusCode::SERVICE_UNAVAILABLE, "Database unavailable, try again later.")
        }
    }
}

async fn submit_listens(State(st): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(token) = token_from(&headers) else {
        return lb_error(StatusCode::UNAUTHORIZED, "You need to provide an Authorization header.");
    };
    let owner = match resolve_token(&st, &token).await {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(error = %e, "submit-listens: token");
            // 503 → Navidrome guarda la escucha y la reintenta.
            return lb_error(StatusCode::SERVICE_UNAVAILABLE, "Database unavailable, try again later.");
        }
    };
    let Some(owner) = owner else {
        if st.cfg.listenbrainz.passthrough_unknown_tokens {
            return forward_raw(&st, reqwest::Method::POST, "submit-listens", "", Some(&token), Some(body)).await;
        }
        return lb_error(StatusCode::UNAUTHORIZED, "Invalid authorization token.");
    };

    let req: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return lb_error(StatusCode::BAD_REQUEST, "Cannot parse JSON document."),
    };
    let listen_type = req.get("listen_type").and_then(Value::as_str).unwrap_or("");
    let Some(payload) = req.get("payload").and_then(Value::as_array) else {
        return lb_error(StatusCode::BAD_REQUEST, "JSON document must contain a payload list.");
    };
    match listen_type {
        "single" | "playing_now" if payload.len() != 1 => {
            return lb_error(
                StatusCode::BAD_REQUEST,
                "JSON document must contain exactly one listen for this listen_type.",
            );
        }
        "import" if payload.is_empty() || payload.len() > MAX_IMPORT => {
            return lb_error(StatusCode::BAD_REQUEST, "Import payload must contain between 1 and 1000 listens.");
        }
        "single" | "playing_now" | "import" => {}
        _ => return lb_error(StatusCode::BAD_REQUEST, "JSON document must contain a valid listen_type."),
    }

    let mut parsed = Vec::with_capacity(payload.len());
    for item in payload {
        match parse_listen(item, listen_type != "playing_now") {
            Ok(l) => parsed.push(l),
            Err(msg) => return lb_error(StatusCode::BAD_REQUEST, msg),
        }
    }

    if listen_type == "playing_now" {
        let l = &parsed[0];
        st.now_playing.write().await.insert(
            owner.user_id,
            NowPlaying {
                artist: l.artist.clone(),
                track: l.track.clone(),
                album: l.album.clone(),
                duration_ms: l.duration_ms,
                client: l.client.clone(),
                since: now(),
            },
        );
        crate::forward::spawn_now_playing(st.clone(), owner.user_id, payload[0].clone());
        crate::forward::spawn_now_playing_lastfm(st.clone(), owner.user_id, parsed[0].clone());
        return axum::Json(json!({"status": "ok"})).into_response();
    }

    let mut stored = 0;
    let turno = st.write_lock.lock().await;
    for l in &parsed {
        match scrobble::ingest(&st.db, owner.user_id, l).await {
            Ok(Outcome::Stored(_)) => stored += 1,
            Ok(Outcome::Duplicate) => tracing::debug!(usuario = %owner.username, pista = %l.track, "duplicada"),
            Ok(Outcome::Ignored(why)) => tracing::debug!(usuario = %owner.username, pista = %l.track, why, "ignorada"),
            Err(e) => {
                tracing::error!(error = %e, "no se pudo guardar la escucha");
                return lb_error(StatusCode::SERVICE_UNAVAILABLE, "Could not store listen, try again later.");
            }
        }
    }
    drop(turno);
    if stored > 0 {
        st.forward_wake.notify_one();
        st.enrich_wake.notify_one();
        // Lo que estaba sonando ya terminó.
        st.now_playing.write().await.remove(&owner.user_id);
    }
    tracing::info!(usuario = %owner.username, recibidas = parsed.len(), guardadas = stored, "escuchas");
    axum::Json(json!({"status": "ok"})).into_response()
}

fn str_field(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Traduce una escucha de ListenBrainz a la nuestra.
pub fn parse_listen(item: &Value, needs_ts: bool) -> Result<Listen, &'static str> {
    let md = item.get("track_metadata").ok_or("Listen must contain track_metadata.")?;
    let artist = str_field(md, "artist_name").ok_or("track_metadata.artist_name must be present.")?;
    let track = str_field(md, "track_name").ok_or("track_metadata.track_name must be present.")?;
    let played_at = match item.get("listened_at") {
        Some(v) => v.as_i64().ok_or("listened_at must be an integer.")?,
        None if needs_ts => return Err("listened_at must be present for this listen_type."),
        None => now(),
    };
    if needs_ts && played_at < MIN_LISTENED_AT {
        return Err("listened_at is too old.");
    }
    if played_at > now() + 600 {
        return Err("listened_at is in the future.");
    }
    let info = md.get("additional_info").cloned().unwrap_or(Value::Null);
    let duration_ms = info
        .get("duration_ms")
        .and_then(Value::as_i64)
        .or_else(|| info.get("duration").and_then(Value::as_i64).map(|s| s * 1000));
    let client = str_field(&info, "submission_client").map(|c| match str_field(&info, "submission_client_version") {
        Some(v) => format!("{c} {v}"),
        None => c,
    });
    Ok(Listen {
        artist,
        track,
        album: str_field(md, "release_name"),
        album_artist: str_field(&info, "release_artist_name").or_else(|| str_field(&info, "albumartist")),
        duration_ms,
        played_at,
        recording_mbid: str_field(&info, "recording_mbid"),
        release_mbid: str_field(&info, "release_mbid"),
        artist_mbid: info
            .get("artist_mbids")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .map(str::to_string),
        client,
        source: Source::Navidrome,
        raw: Some(item.clone()),
        cover_url: None,
    })
}

async fn passthrough_get(State(st): State<AppState>, headers: HeaderMap, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches("/1/").to_string();
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    forward_raw(&st, reqwest::Method::GET, &path, &query, token_from(&headers).as_deref(), None).await
}

/// Reenvía la petición tal cual a la API real de ListenBrainz y devuelve su respuesta.
async fn forward_raw(
    st: &AppState,
    method: reqwest::Method,
    path: &str,
    query: &str,
    token: Option<&str>,
    body: Option<Bytes>,
) -> Response {
    let url = format!("{}{}", st.lb.url(path), query);
    let mut req = st.http.request(method, &url);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Token {t}"));
    }
    if let Some(b) = body {
        req = req.header("Content-Type", "application/json").body(b);
    }
    match req.send().await {
        Ok(r) => {
            let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let ctype =
                r.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| HeaderValue::from_bytes(v.as_bytes()).ok());
            let bytes = r.bytes().await.unwrap_or_default();
            let mut resp = Response::new(Body::from(bytes));
            *resp.status_mut() = status;
            if let Some(ct) = ctype {
                resp.headers_mut().insert(header::CONTENT_TYPE, ct);
            }
            resp
        }
        Err(e) => {
            tracing::warn!(error = %e, url, "no se pudo pasar la petición a ListenBrainz");
            // 503: si era una escucha, Navidrome la reintentará.
            lb_error(StatusCode::SERVICE_UNAVAILABLE, "Upstream ListenBrainz unavailable.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduce_escucha_de_navidrome() {
        let item = json!({
            "listened_at": 1_700_000_000,
            "track_metadata": {
                "artist_name": "Rosalía", "track_name": "Saoko", "release_name": "Motomami",
                "additional_info": {
                    "submission_client": "Navidrome", "submission_client_version": "0.64.2",
                    "duration_ms": 137000, "recording_mbid": "rec", "release_mbid": "rel",
                    "artist_mbids": ["art"], "tracknumber": 1
                }
            }
        });
        let l = parse_listen(&item, true).unwrap();
        assert_eq!((l.artist.as_str(), l.track.as_str(), l.album.as_deref()), ("Rosalía", "Saoko", Some("Motomami")));
        assert_eq!(l.duration_ms, Some(137000));
        assert_eq!(l.played_at, 1_700_000_000);
        assert_eq!(l.client.as_deref(), Some("Navidrome 0.64.2"));
        assert_eq!(l.artist_mbid.as_deref(), Some("art"));
    }

    #[test]
    fn rechaza_escuchas_incompletas() {
        assert!(parse_listen(&json!({"track_metadata": {"artist_name": "A", "track_name": "B"}}), true).is_err());
        assert!(
            parse_listen(&json!({"listened_at": 1_700_000_000, "track_metadata": {"artist_name": "A"}}), true).is_err()
        );
        assert!(
            parse_listen(&json!({"listened_at": 5, "track_metadata": {"artist_name": "A", "track_name": "B"}}), true)
                .is_err()
        );
        // playing_now no necesita listened_at
        assert!(parse_listen(&json!({"track_metadata": {"artist_name": "A", "track_name": "B"}}), false).is_ok());
    }
}
