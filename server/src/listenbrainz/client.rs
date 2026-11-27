//! Cliente de la API real de ListenBrainz (para reenviar escuchas y comprobar tokens).

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct ListenBrainz {
    http: reqwest::Client,
    api: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SubmitError {
    /// Fallo temporal (red, 5xx, 429). `after` = segundos que pide esperar.
    RetryLater { after: Option<i64>, msg: String },
    /// Token rechazado: hay que reconectar.
    Auth(String),
    /// La escucha no es válida para ListenBrainz: no se reintenta.
    Permanent(String),
}

impl ListenBrainz {
    pub fn new(http: reqwest::Client, api: &str) -> Self {
        Self { http, api: api.trim_end_matches('/').to_string() }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}/1/{}", self.api, path.trim_start_matches('/'))
    }

    /// Devuelve `Ok(Some(usuario))` si el token es válido, `Ok(None)` si no.
    pub async fn validate_token(&self, token: &str) -> Result<Option<String>, String> {
        let r = self
            .http
            .get(self.url("validate-token"))
            .header("Authorization", format!("Token {token}"))
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("no se pudo contactar con ListenBrainz: {}", e.without_url()))?;
        let status = r.status();
        let body: Value = r.json().await.map_err(|e| format!("respuesta rara de ListenBrainz (HTTP {status}): {e}"))?;
        if body.get("valid").and_then(Value::as_bool) == Some(true) {
            Ok(body.get("user_name").and_then(Value::as_str).map(str::to_string).or(Some(String::new())))
        } else if status.is_success() || status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST {
            Ok(None)
        } else {
            Err(format!("ListenBrainz respondió HTTP {status}"))
        }
    }

    pub async fn submit(&self, token: &str, listen_type: &str, payload: Vec<Value>) -> Result<(), SubmitError> {
        let body = json!({"listen_type": listen_type, "payload": payload});
        let r = match self
            .http
            .post(self.url("submit-listens"))
            .header("Authorization", format!("Token {token}"))
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return Err(SubmitError::RetryLater { after: None, msg: format!("red: {e}") }),
        };
        let status = r.status();
        if status.is_success() {
            return Ok(());
        }
        let after = header_secs(&r, "retry-after").or_else(|| header_secs(&r, "x-ratelimit-reset-in"));
        let text = r.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| text.chars().take(200).collect());
        let msg = format!("HTTP {}: {msg}", status.as_u16());
        Err(match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => SubmitError::Auth(msg),
            StatusCode::TOO_MANY_REQUESTS => SubmitError::RetryLater { after, msg },
            s if s.is_server_error() => SubmitError::RetryLater { after, msg },
            _ => SubmitError::Permanent(msg),
        })
    }
}

fn header_secs(r: &reqwest::Response, name: &str) -> Option<i64> {
    r.headers().get(name)?.to_str().ok()?.trim().parse::<i64>().ok()
}
