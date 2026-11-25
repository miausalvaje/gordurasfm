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
