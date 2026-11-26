//! Cliente mínimo de la API Subsonic de Navidrome (autenticación por token + salt).

use serde::Deserialize;
use serde_json::Value;

pub const CLIENT_NAME: &str = "gordurasfm";
pub const API_VERSION: &str = "1.16.1";

/// Credenciales de Subsonic de un usuario: `t = md5(contraseña + s)`.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct Creds {
    pub u: String,
    pub t: String,
    pub s: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SubsonicError {
    /// Usuario o contraseña incorrectos (códigos 40/41/44 de Subsonic).
    #[error("usuario o contraseña incorrectos")]
    BadCredentials,
    #[error("Navidrome respondió con un error: {0}")]
    Api(String),
    #[error("no se pudo contactar con Navidrome: {0}")]
    Network(String),
}

#[derive(Clone)]
pub struct Subsonic {
    http: reqwest::Client,
    base: String,
}

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub username: String,
    pub admin: bool,
}

impl Subsonic {
    pub fn new(http: reqwest::Client, base: &str) -> Self {
        Self { http, base: base.trim_end_matches('/').to_string() }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// Llama a `/rest/<método>` y devuelve el contenido de `subsonic-response` si el estado es `ok`.
    pub async fn call(&self, creds: &Creds, method: &str, params: &[(&str, &str)]) -> Result<Value, SubsonicError> {
        let url = format!("{}/rest/{}", self.base, method);
        let mut q: Vec<(&str, &str)> = vec![
            ("u", &creds.u),
            ("t", &creds.t),
            ("s", &creds.s),
            ("v", API_VERSION),
            ("c", CLIENT_NAME),
            ("f", "json"),
        ];
        q.extend_from_slice(params);
        let resp = self.http.get(&url).query(&q).send().await.map_err(|e| SubsonicError::Network(e.to_string()))?;
        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| SubsonicError::Network(format!("HTTP {status}: {e}")))?;
        let r = body
            .get("subsonic-response")
            .cloned()
            .ok_or_else(|| SubsonicError::Api("respuesta sin subsonic-response".into()))?;
        if r.get("status").and_then(Value::as_str) == Some("ok") {
            return Ok(r);
        }
        let code = r.pointer("/error/code").and_then(Value::as_i64).unwrap_or(0);
        let msg = r.pointer("/error/message").and_then(Value::as_str).unwrap_or("desconocido").to_string();
        if matches!(code, 40 | 41 | 44) {
            Err(SubsonicError::BadCredentials)
        } else {
            Err(SubsonicError::Api(format!("{code}: {msg}")))
        }
    }

    /// Llama a un método que devuelve binario (`getCoverArt`, `stream`). Devuelve (tipo, bytes).
    pub async fn bytes(
        &self,
        creds: &Creds,
        method: &str,
        params: &[(&str, &str)],
    ) -> Result<(String, bytes::Bytes), SubsonicError> {
        let url = format!("{}/rest/{}", self.base, method);
        let mut q: Vec<(&str, &str)> = vec![
            ("u", &creds.u),
            ("t", &creds.t),
            ("s", &creds.s),
            ("v", API_VERSION),
            ("c", CLIENT_NAME),
            ("f", "json"),
        ];
        q.extend_from_slice(params);
        let resp = self.http.get(&url).query(&q).send().await.map_err(|e| SubsonicError::Network(e.to_string()))?;
        let status = resp.status();
        let ctype = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let body = resp.bytes().await.map_err(|e| SubsonicError::Network(e.to_string()))?;
        if ctype.contains("json") || ctype.contains("xml") || !status.is_success() {
            let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let code = v.pointer("/subsonic-response/error/code").and_then(Value::as_i64).unwrap_or(0);
            let msg = v
                .pointer("/subsonic-response/error/message")
                .and_then(Value::as_str)
                .unwrap_or("sin imagen")
                .to_string();
            return Err(if matches!(code, 40 | 41 | 44) {
                SubsonicError::BadCredentials
            } else {
                SubsonicError::Api(format!("{code}: {msg}"))
            });
        }
        Ok((ctype, body))
    }

    /// `stream`: devuelve la respuesta tal cual (para reenviarla en streaming, con `Range`).
    pub async fn stream(
        &self,
        creds: &Creds,
        params: &[(&str, &str)],
        range: Option<&str>,
    ) -> Result<reqwest::Response, SubsonicError> {
        let url = format!("{}/rest/stream", self.base);
        let mut q: Vec<(&str, &str)> = vec![
            ("u", &creds.u),
            ("t", &creds.t),
            ("s", &creds.s),
            ("v", API_VERSION),
            ("c", CLIENT_NAME),
            ("f", "json"),
        ];
        q.extend_from_slice(params);
        let mut req = self.http.get(&url).query(&q);
        if let Some(r) = range {
            req = req.header(reqwest::header::RANGE, r);
        }
        let resp = req.send().await.map_err(|e| SubsonicError::Network(e.to_string()))?;
        let ctype = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        if !resp.status().is_success() || ctype.contains("json") || ctype.contains("xml") {
            return Err(SubsonicError::Api(format!("stream: HTTP {}", resp.status())));
        }
        Ok(resp)
    }

    /// `search3`: devuelve (artistas, álbumes, canciones).
    pub async fn search3(
        &self,
        creds: &Creds,
        query: &str,
        artists: u32,
        albums: u32,
        songs: u32,
    ) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>), SubsonicError> {
        let (a, b, c) = (artists.to_string(), albums.to_string(), songs.to_string());
        let r = self
            .call(creds, "search3", &[("query", query), ("artistCount", &a), ("albumCount", &b), ("songCount", &c)])
            .await?;
        let list = |k: &str| match r.pointer(&format!("/searchResult3/{k}")) {
            Some(Value::Array(v)) => v.clone(),
            Some(o @ Value::Object(_)) => vec![o.clone()],
            _ => vec![],
        };
        Ok((list("artist"), list("album"), list("song")))
    }

    /// Comprueba las credenciales y devuelve los datos del usuario.
    pub async fn login(&self, creds: &Creds) -> Result<UserInfo, SubsonicError> {
        self.call(creds, "ping", &[]).await?;
        let r = self.call(creds, "getUser", &[("username", &creds.u)]).await?;
        let user = r.get("user").cloned().unwrap_or(Value::Null);
        Ok(UserInfo {
            username: user.get("username").and_then(Value::as_str).unwrap_or(&creds.u).to_string(),
            admin: user.get("adminRole").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}
