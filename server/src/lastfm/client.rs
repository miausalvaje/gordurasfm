//! Cliente de la API de Last.fm (firma de peticiones, sesión, escuchas recientes, scrobble).

use std::collections::BTreeMap;

use md5::{Digest, Md5};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Key {
    pub api_key: String,
    pub secret: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LfmError {
    /// Key, sesión o token no válidos: hay que reconectar o revisar la key.
    Auth(String),
    /// Usuario o recurso inexistente.
    NotFound(String),
    /// Fallo temporal (red, servicio caído, límite de peticiones).
    Retry(String),
    Permanent(String),
}

impl std::fmt::Display for LfmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LfmError::Auth(m) => write!(f, "Last.fm rechaza la autorización: {m}"),
            LfmError::NotFound(m) => write!(f, "Last.fm: {m}"),
            LfmError::Retry(m) => write!(f, "Last.fm no está disponible ahora: {m}"),
            LfmError::Permanent(m) => write!(f, "Last.fm: {m}"),
        }
    }
}

#[derive(Clone)]
pub struct LastFm {
    http: reqwest::Client,
    api: String,
}

#[derive(Debug)]
pub struct RecentPage {
    pub tracks: Vec<Value>,
    pub page: i64,
    pub total_pages: i64,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct ScrobbleItem {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub timestamp: i64,
    pub duration_secs: Option<i64>,
}

/// Resultado de cada escucha enviada: `Ok` aceptada, `Err((código, mensaje))` ignorada por Last.fm.
pub type ScrobbleResult = Result<(), (i64, String)>;

/// `api_sig` = md5 de los parámetros ordenados (`clavevalor…`) + secret, sin `format` ni `callback`.
pub fn sign(params: &BTreeMap<String, String>, secret: &str) -> String {
    let mut h = Md5::new();
    for (k, v) in params {
        if k == "format" || k == "callback" {
            continue;
        }
        h.update(k.as_bytes());
        h.update(v.as_bytes());
    }
    h.update(secret.as_bytes());
    hex::encode(h.finalize())
}

fn classify(code: i64, msg: String) -> LfmError {
    match code {
        4 | 9 | 10 | 14 | 15 | 26 => LfmError::Auth(msg),
        6 | 7 => LfmError::NotFound(msg),
        8 | 11 | 16 | 29 => LfmError::Retry(msg),
        _ => LfmError::Permanent(format!("error {code}: {msg}")),
    }
}

fn as_list(v: Option<&Value>) -> Vec<Value> {
    match v {
        Some(Value::Array(a)) => a.clone(),
        Some(o @ Value::Object(_)) => vec![o.clone()],
        _ => vec![],
    }
}

fn num(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

impl LastFm {
    pub fn new(http: reqwest::Client, api: &str) -> Self {
        Self { http, api: api.to_string() }
    }

    async fn call(
        &self,
        method: &str,
        mut params: BTreeMap<String, String>,
        key: &Key,
        signed: bool,
        post: bool,
    ) -> Result<Value, LfmError> {
        params.insert("method".into(), method.into());
        params.insert("api_key".into(), key.api_key.clone());
        if signed {
            let sig = sign(&params, &key.secret);
            params.insert("api_sig".into(), sig);
        }
        params.insert("format".into(), "json".into());
        let req = if post { self.http.post(&self.api).form(&params) } else { self.http.get(&self.api).query(&params) };
        let resp = req.send().await.map_err(|e| LfmError::Retry(format!("red: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| LfmError::Retry(format!("red: {e}")))?;
        let v: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) if status.is_server_error() => return Err(LfmError::Retry(format!("HTTP {status}"))),
            Err(_) => return Err(LfmError::Permanent(format!("respuesta no válida (HTTP {status})"))),
        };
        if let Some(code) = v.get("error").and_then(Value::as_i64) {
            let msg = v.get("message").and_then(Value::as_str).unwrap_or("").to_string();
            return Err(classify(code, msg));
        }
        if status.is_server_error() {
            return Err(LfmError::Retry(format!("HTTP {status}")));
        }
        Ok(v)
    }

    /// Cambia el token del flujo web por una session key (no caduca). Devuelve (usuario, sk).
    pub async fn get_session(&self, key: &Key, token: &str) -> Result<(String, String), LfmError> {
        let mut p = BTreeMap::new();
        p.insert("token".to_string(), token.to_string());
        let v = self.call("auth.getSession", p, key, true, false).await?;
        let name = v.pointer("/session/name").and_then(Value::as_str);
        let sk = v.pointer("/session/key").and_then(Value::as_str);
        match (name, sk) {
            (Some(n), Some(k)) => Ok((n.to_string(), k.to_string())),
            _ => Err(LfmError::Permanent("respuesta de sesión incompleta".into())),
        }
    }

    pub async fn user_exists(&self, key: &Key, user: &str) -> Result<String, LfmError> {
        let mut p = BTreeMap::new();
        p.insert("user".to_string(), user.to_string());
        let v = self.call("user.getInfo", p, key, false, false).await?;
        Ok(v.pointer("/user/name").and_then(Value::as_str).unwrap_or(user).to_string())
    }

    /// Una página de `user.getRecentTracks` (de lo más nuevo a lo más viejo).
    pub async fn recent_tracks(
        &self,
        key: &Key,
        user: &str,
        page: i64,
        limit: i64,
        from: Option<i64>,
        to: Option<i64>,
    ) -> Result<RecentPage, LfmError> {
        let mut p = BTreeMap::new();
        p.insert("user".to_string(), user.to_string());
        p.insert("page".to_string(), page.to_string());
        p.insert("limit".to_string(), limit.to_string());
        if let Some(f) = from {
            p.insert("from".to_string(), f.to_string());
        }
        if let Some(t) = to {
            p.insert("to".to_string(), t.to_string());
        }
        let v = self.call("user.getRecentTracks", p, key, false, false).await?;
        let rt = v.get("recenttracks").ok_or_else(|| LfmError::Permanent("respuesta sin recenttracks".into()))?;
        let attr = rt.get("@attr");
        Ok(RecentPage {
            tracks: as_list(rt.get("track")),
            page: num(attr.and_then(|a| a.get("page"))),
            total_pages: num(attr.and_then(|a| a.get("totalPages"))),
            total: num(attr.and_then(|a| a.get("total"))),
        })
    }

    /// `track.scrobble` en lote (hasta 50).
    pub async fn scrobble(&self, key: &Key, sk: &str, items: &[ScrobbleItem]) -> Result<Vec<ScrobbleResult>, LfmError> {
        let mut p = BTreeMap::new();
        p.insert("sk".to_string(), sk.to_string());
        for (i, it) in items.iter().enumerate() {
            p.insert(format!("artist[{i}]"), it.artist.clone());
            p.insert(format!("track[{i}]"), it.track.clone());
            p.insert(format!("timestamp[{i}]"), it.timestamp.to_string());
            if let Some(a) = &it.album {
                p.insert(format!("album[{i}]"), a.clone());
            }
            if let Some(a) = &it.album_artist {
                p.insert(format!("albumArtist[{i}]"), a.clone());
            }
            if let Some(d) = it.duration_secs {
                p.insert(format!("duration[{i}]"), d.to_string());
            }
        }
        let v = self.call("track.scrobble", p, key, true, true).await?;
        let list = as_list(v.pointer("/scrobbles/scrobble"));
        if list.len() != items.len() {
            // Sin detalle por escucha: si Last.fm dice cuántas aceptó y son todas, vale.
            let accepted = num(v.pointer("/scrobbles/@attr/accepted"));
            return Ok(items
                .iter()
                .map(
                    |_| {
                        if accepted as usize == items.len() { Ok(()) } else { Err((0, "respuesta sin detalle".into())) }
                    },
                )
                .collect());
        }
        Ok(list
            .iter()
            .map(|s| {
                let code = num(s.pointer("/ignoredMessage/code"));
                if code == 0 {
                    Ok(())
                } else {
                    let msg = s.pointer("/ignoredMessage/#text").and_then(Value::as_str).unwrap_or("").to_string();
                    Err((code, msg))
                }
            })
            .collect())
    }

    pub async fn now_playing(&self, key: &Key, sk: &str, it: &ScrobbleItem) -> Result<(), LfmError> {
        let mut p = BTreeMap::new();
        p.insert("sk".to_string(), sk.to_string());
        p.insert("artist".to_string(), it.artist.clone());
        p.insert("track".to_string(), it.track.clone());
        if let Some(a) = &it.album {
            p.insert("album".to_string(), a.clone());
        }
        if let Some(d) = it.duration_secs {
            p.insert("duration".to_string(), d.to_string());
        }
        self.call("track.updateNowPlaying", p, key, true, true).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firma_ordenada_sin_format() {
        let mut p = BTreeMap::new();
        p.insert("method".to_string(), "auth.getSession".to_string());
        p.insert("api_key".to_string(), "k".to_string());
        p.insert("token".to_string(), "t".to_string());
        p.insert("format".to_string(), "json".to_string());
        // md5("api_keykmethodauth.getSessiontokentSECRETO")
        let expected = hex::encode(Md5::digest(b"api_keykmethodauth.getSessiontokentSECRETO"));
        assert_eq!(sign(&p, "SECRETO"), expected);
    }

    #[test]
    fn clasifica_errores() {
        assert!(matches!(classify(9, "x".into()), LfmError::Auth(_)));
        assert!(matches!(classify(29, "x".into()), LfmError::Retry(_)));
        assert!(matches!(classify(6, "x".into()), LfmError::NotFound(_)));
        assert!(matches!(classify(13, "x".into()), LfmError::Permanent(_)));
    }
}
