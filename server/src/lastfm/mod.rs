//! Last.fm: keys (instancia o propia del usuario), conexión de la cuenta y traducción de escuchas.

pub mod client;

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    auth::CurrentUser,
    db::now,
    error::{AppError, AppResult},
    scrobble::{Listen, Source},
    settings,
    state::AppState,
};
use client::{Key, LfmError};

/// Mensaje cuando la key no tiene secret.
pub const NO_SECRET: &str =
    "la key de Last.fm de esta instancia no tiene secret: sirve para importar, pero para conectar tu cuenta hace falta también el secret (admin) o tu propia key";

/// Placeholder de Last.fm cuando no hay portada (no vale la pena guardarlo).
const NO_IMAGE: &str = "2a96cbd8b46e442fc41c2b86b821562f";
/// Hosts de los que se aceptan portadas de Last.fm (para no descargar cualquier URL).
pub const IMAGE_HOSTS: &[&str] = &[
    "lastfm.freetls.fastly.net",
    "lastfm-img.freetls.fastly.net",
    "lastfm-img2.akamaized.net",
    "userserve-ak.last.fm",
];

/// Key de la instancia: configuración o ajustes de la BD.
///
/// El secret es opcional: sin él la key sirve para importar historiales (lecturas sin firmar), pero
/// no para conectar cuentas ni reenviar escuchas (`Key::can_sign`).
pub async fn instance_key(st: &AppState) -> anyhow::Result<Option<Key>> {
    if let Some(k) = &st.cfg.lastfm.api_key {
        let secret = st.cfg.lastfm.api_secret.clone().unwrap_or_default();
        return Ok(Some(Key { api_key: k.clone(), secret }));
    }
    let k = settings::get(&st.db, "lastfm_api_key").await?;
    let s = settings::get_secret(&st.db, &st.crypto, "lastfm_api_secret").await?;
    Ok(k.map(|api_key| Key { api_key, secret: s.unwrap_or_default() }))
}

/// Key propia del usuario, si la tiene.
pub async fn user_key(st: &AppState, user_id: i64) -> anyhow::Result<Option<Key>> {
    let row: Option<(String, Vec<u8>)> =
        sqlx::query_as("SELECT api_key, api_secret_enc FROM lastfm_user_keys WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(&st.db)
            .await?;
    Ok(match row {
        Some((api_key, enc)) => Some(Key { api_key, secret: st.crypto.decrypt_str(&enc)? }),
        None => None,
    })
}

/// Key para un usuario: la suya o, si no tiene, la de la instancia.
pub async fn key_for(st: &AppState, user_id: i64) -> anyhow::Result<Option<Key>> {
    Ok(match user_key(st, user_id).await? {
        Some(k) => Some(k),
        None => instance_key(st).await?,
    })
}

/// Key y session key de la conexión de Last.fm del usuario (la session va ligada a la key que la creó).
pub async fn session_for(st: &AppState, user_id: i64, require_forward: bool) -> anyhow::Result<Option<(Key, String)>> {
    let row: Option<(Vec<u8>, Option<String>)> = sqlx::query_as(
        "SELECT secret_enc, own_api_key FROM connections WHERE user_id = ? AND provider = 'lastfm' AND status = 'ok' \
         AND (forward_enabled = 1 OR ? = 0)",
    )
    .bind(user_id)
    .bind(require_forward)
    .fetch_optional(&st.db)
    .await?;
    let Some((enc, own)) = row else { return Ok(None) };
    let sk = st.crypto.decrypt_str(&enc)?;
    let key = match own {
        Some(_) => user_key(st, user_id).await?,
        None => instance_key(st).await?,
    };
    Ok(key.map(|k| (k, sk)))
}

/// Traduce una pista de `user.getRecentTracks`. `None` si es la que suena ahora o le faltan datos.
pub fn listen_from_recent(t: &Value) -> Option<Listen> {
    if t.pointer("/@attr/nowplaying").and_then(Value::as_str) == Some("true") {
        return None;
    }
    let text = |v: Option<&Value>| -> Option<String> {
        let v = v?;
        let s = v.get("#text").or_else(|| v.get("name")).and_then(Value::as_str).or_else(|| v.as_str())?;
        let s = s.trim();
        (!s.is_empty()).then(|| s.to_string())
    };
    let artist = text(t.get("artist"))?;
    let track = t.get("name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())?.to_string();
    let uts: i64 = t.pointer("/date/uts").and_then(|v| v.as_str().map(|s| s.parse().ok()).unwrap_or(v.as_i64()))?;
    let album = text(t.get("album"));
    let mbid = |v: Option<&Value>| {
        v.and_then(|x| x.get("mbid")).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
    };
    let image = t
        .get("image")
        .and_then(Value::as_array)
        .and_then(|imgs| {
            imgs.iter()
                .rev()
                .find_map(|i| i.get("#text").and_then(Value::as_str).filter(|u| !u.is_empty()).map(str::to_string))
        })
        .filter(|u| !u.contains(NO_IMAGE));
    let rec_mbid = t.get("mbid").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    Some(Listen {
        raw: Some(
            json!({"artist": artist, "track": track, "album": album, "uts": uts, "mbid": rec_mbid, "image": image}),
        ),
        artist_mbid: mbid(t.get("artist")),
        release_mbid: mbid(t.get("album")),
        recording_mbid: rec_mbid,
        artist,
        track,
        album,
        album_artist: None,
        duration_ms: None,
        played_at: uts,
        client: Some("Last.fm".into()),
        source: Source::Lastfm,
        cover_url: image,
    })
}

/// GET /api/v1/connections/lastfm/start — URL de Last.fm a la que mandar al usuario para autorizar.
pub async fn start(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Value>> {
    let key = key_for(&st, u.id)
        .await?
        .ok_or_else(|| AppError::BadRequest("falta configurar la API key de Last.fm (admin o tu propia key)".into()))?;
    if !key.can_sign() {
        return Err(AppError::BadRequest(NO_SECRET.into()));
    }
    let state = crate::crypto::random_token();
    {
        let mut m = st.lastfm_pending.lock().unwrap();
        m.retain(|_, (_, t)| t.elapsed().as_secs() < 900);
        m.insert(state.clone(), (u.id, std::time::Instant::now()));
    }
    let cb = format!("{}/api/v1/connections/lastfm/callback?state={state}", st.cfg.public_url);
    let url = format!("{}?api_key={}&cb={}", st.cfg.lastfm.auth_url, key.api_key, urlencode(&cb));
    Ok(Json(json!({"url": url})))
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub state: Option<String>,
    pub token: Option<String>,
}

fn redirect(to: &str) -> Response {
    let mut r = StatusCode::SEE_OTHER.into_response();
    r.headers_mut().insert(header::LOCATION, HeaderValue::from_str(to).unwrap_or(HeaderValue::from_static("/")));
    r
}

/// GET /api/v1/connections/lastfm/callback — Last.fm vuelve aquí con `token`.
pub async fn callback(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let back = "/ajustes/conexiones";
    let ok_state = q
        .state
        .as_ref()
        .and_then(|s| st.lastfm_pending.lock().unwrap().remove(s))
        .filter(|(uid, t)| *uid == u.id && t.elapsed().as_secs() < 900);
    if ok_state.is_none() {
        return redirect(&format!("{back}?lastfm=error&motivo=estado"));
    }
    let Some(token) = q.token.filter(|t| !t.is_empty()) else {
        return redirect(&format!("{back}?lastfm=error&motivo=cancelado"));
    };
    match connect_with_token(&st, u.id, &token).await {
        Ok(_) => redirect(&format!("{back}?lastfm=ok")),
        Err(e) => {
            tracing::warn!(error = %e, "conexión con Last.fm");
            redirect(&format!("{back}?lastfm=error&motivo=sesion"))
        }
    }
}

async fn connect_with_token(st: &AppState, user_id: i64, token: &str) -> anyhow::Result<String> {
    let own = user_key(st, user_id).await?;
    let key = match &own {
        Some(k) => k.clone(),
        None => instance_key(st).await?.ok_or_else(|| anyhow::anyhow!("sin key de Last.fm"))?,
    };
    let (name, sk) = st.lastfm.get_session(&key, token).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    let t = now();
    let mut tx = st.db.begin().await?;
    // El reenvío a Last.fm empieza desactivado: lo activa el usuario si quiere.
    sqlx::query(
        "INSERT INTO connections (user_id, provider, remote_username, secret_enc, own_api_key, forward_enabled, status, last_error, updated_at) \
         VALUES (?, 'lastfm', ?, ?, ?, 0, 'ok', NULL, ?) \
         ON CONFLICT(user_id, provider) DO UPDATE SET remote_username = excluded.remote_username, secret_enc = excluded.secret_enc, \
           own_api_key = excluded.own_api_key, status = 'ok', last_error = NULL, updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(&name)
    .bind(st.crypto.encrypt_str(&sk))
    .bind(own.map(|k| k.api_key))
    .bind(t)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE forward_queue SET status = 'pending', next_attempt_at = ?, updated_at = ? \
         WHERE user_id = ? AND target = 'lastfm' AND status = 'paused'",
    )
    .bind(t)
    .bind(t)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    st.forward_wake.notify_one();
    tracing::info!(usuario = user_id, lastfm = %name, "Last.fm conectado");
    Ok(name)
}

#[derive(Deserialize)]
pub struct OwnKey {
    pub api_key: String,
    pub api_secret: String,
}

/// PUT /api/v1/connections/lastfm/key — key y secret propios del usuario.
pub async fn put_own_key(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Json(req): Json<OwnKey>,
) -> AppResult<Json<Value>> {
    let (k, s) = (req.api_key.trim(), req.api_secret.trim());
    if k.len() != 32 || s.len() != 32 || !k.chars().chain(s.chars()).all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest("la key y el secret de Last.fm son 32 caracteres hexadecimales".into()));
    }
    // Comprueba la key con una llamada pública.
    let key = Key { api_key: k.to_string(), secret: s.to_string() };
    match st.lastfm.user_exists(&key, "rj").await {
        Ok(_) | Err(LfmError::NotFound(_)) => {}
        Err(LfmError::Auth(m)) => return Err(AppError::BadRequest(format!("Last.fm no acepta esa key: {m}"))),
        Err(e) => return Err(AppError::BadGateway(e.to_string())),
    }
    sqlx::query(
        "INSERT INTO lastfm_user_keys (user_id, api_key, api_secret_enc, updated_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(user_id) DO UPDATE SET api_key = excluded.api_key, api_secret_enc = excluded.api_secret_enc, updated_at = excluded.updated_at",
    )
    .bind(u.id)
    .bind(k)
    .bind(st.crypto.encrypt_str(s))
    .bind(now())
    .execute(&st.db)
    .await?;
    Ok(Json(json!({"ok": true, "note": "si ya tenías Last.fm conectado, vuelve a conectarlo para usar esta key"})))
}

/// DELETE /api/v1/connections/lastfm/key
pub async fn delete_own_key(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM lastfm_user_keys WHERE user_id = ?").bind(u.id).execute(&st.db).await?;
    Ok(Json(json!({"ok": true})))
}

/// GET /api/v1/lastfm/status — si hay key disponible para este usuario.
pub async fn status(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Value>> {
    let own = user_key(&st, u.id).await?.is_some();
    let instance = instance_key(&st).await?.is_some();
    Ok(Json(json!({"own_key": own, "instance_key": instance, "available": own || instance})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduce_pista_reciente() {
        let t = json!({
            "artist": {"mbid": "a1", "#text": "Rosalía"},
            "name": "Saoko", "mbid": "",
            "album": {"mbid": "", "#text": "Motomami"},
            "image": [{"size": "small", "#text": "https://lastfm.freetls.fastly.net/i/u/34s/x.jpg"},
                      {"size": "extralarge", "#text": "https://lastfm.freetls.fastly.net/i/u/300x300/x.jpg"}],
            "date": {"uts": "1700000000", "#text": "14 Nov 2023, 22:13"}
        });
        let l = listen_from_recent(&t).unwrap();
        assert_eq!((l.artist.as_str(), l.track.as_str(), l.album.as_deref()), ("Rosalía", "Saoko", Some("Motomami")));
        assert_eq!(l.played_at, 1_700_000_000);
        assert_eq!(l.artist_mbid.as_deref(), Some("a1"));
        assert_eq!(l.recording_mbid, None);
        assert_eq!(l.cover_url.as_deref(), Some("https://lastfm.freetls.fastly.net/i/u/300x300/x.jpg"));
        assert_eq!(l.source, Source::Lastfm);
    }

    #[test]
    fn ignora_la_que_suena_y_el_placeholder() {
        let np = json!({"artist": {"#text": "A"}, "name": "B", "@attr": {"nowplaying": "true"}});
        assert!(listen_from_recent(&np).is_none());
        let t = json!({"artist": {"name": "A"}, "name": "B", "date": {"uts": "1700000000"},
            "image": [{"size": "extralarge", "#text": "https://lastfm.freetls.fastly.net/i/u/300x300/2a96cbd8b46e442fc41c2b86b821562f.png"}]});
        let l = listen_from_recent(&t).unwrap();
        assert_eq!(l.artist, "A", "formato extended (artist.name)");
        assert!(l.cover_url.is_none());
    }

    #[test]
    fn codifica_url() {
        assert_eq!(urlencode("http://a/b?state=x"), "http%3A%2F%2Fa%2Fb%3Fstate%3Dx");
    }
}
