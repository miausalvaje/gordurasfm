//! Inicio de sesión con las credenciales de Navidrome, sesiones propias y usuario actual.

use axum::{
    Json,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, Method, header, request::Parts},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{
    crypto,
    db::now,
    error::{AppError, AppResult},
    state::AppState,
    subsonic::{Creds, SubsonicError},
};

pub const SESSION_COOKIE: &str = "gfm_session";
pub const CSRF_HEADER: &str = "x-requested-with";
const SESSION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub is_admin: bool,
    pub is_approved: bool,
    pub timezone: String,
    pub is_public: bool,
    pub created_at: i64,
}

#[derive(FromRow)]
struct SessionRow {
    #[sqlx(flatten)]
    user: User,
    last_seen_at: i64,
}

const USER_COLS: &str =
    "u.id, u.username, u.display_name, u.is_admin, u.is_approved, u.timezone, u.is_public, u.created_at";

/// Usuario autenticado (por cookie de sesión o por token de API `Bearer`).
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        // 1) Token de API (integraciones): Authorization: Bearer <token>
        if let Some(tok) = bearer(&parts.headers) {
            let h = crypto::token_hash(&tok);
            let sql = format!(
                "SELECT {USER_COLS} FROM api_tokens t JOIN users u ON u.id = t.user_id WHERE t.token_hash = ? AND t.kind = 'api'"
            );
            let user: Option<User> = sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(&h).fetch_optional(&st.db).await?;
            let user = user.ok_or(AppError::Unauthorized)?;
            sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE token_hash = ?")
                .bind(now())
                .bind(&h)
                .execute(&st.db)
                .await?;
            return approved(user);
        }

        // 2) Cookie de sesión
        let tok = cookie(&parts.headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
        // Protección CSRF: las peticiones que cambian algo deben traer la cabecera propia.
        if !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS)
            && parts.headers.get(CSRF_HEADER).is_none()
        {
            return Err(AppError::Forbidden("falta la cabecera X-Requested-With".into()));
        }
        let h = crypto::token_hash(&tok);
        let t = now();
        let sql = format!(
            "SELECT {USER_COLS}, s.last_seen_at FROM sessions s JOIN users u ON u.id = s.user_id \
             WHERE s.token_hash = ? AND s.expires_at > ?"
        );
        let row: Option<SessionRow> =
            sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(&h).bind(t).fetch_optional(&st.db).await?;
        let SessionRow { user, last_seen_at } = row.ok_or(AppError::Unauthorized)?;
        // Sesión deslizante: se renueva como mucho una vez por hora.
        if t - last_seen_at > 3600 {
            sqlx::query("UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE token_hash = ?")
                .bind(t)
                .bind(t + SESSION_DAYS * 86400)
                .bind(&h)
                .execute(&st.db)
                .await?;
        }
        approved(user)
    }
}

fn approved(user: User) -> Result<CurrentUser, AppError> {
    if !user.is_approved {
        return Err(AppError::Forbidden("tu cuenta está pendiente de que la apruebe un administrador".into()));
    }
    Ok(CurrentUser(user))
}

pub fn bearer(h: &HeaderMap) -> Option<String> {
    let v = h.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, tok) = v.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| tok.trim().to_string()).filter(|t| !t.is_empty())
}

pub fn cookie(h: &HeaderMap, name: &str) -> Option<String> {
    h.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|kv| {
        let (k, v) = kv.trim().split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

fn session_cookie(st: &AppState, value: &str, max_age: i64) -> HeaderValue {
    let secure = if st.cfg.secure_cookies() { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("cookie válida")
}

#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResp {
    pub user: User,
}

/// POST /api/v1/auth/login
pub async fn login(State(st): State<AppState>, headers: HeaderMap, Json(req): Json<LoginReq>) -> AppResult<Response> {
    if headers.get(CSRF_HEADER).is_none() {
        return Err(AppError::Forbidden("falta la cabecera X-Requested-With".into()));
    }
    let username = req.username.trim().to_string();
    if username.is_empty() || req.password.is_empty() {
        return Err(AppError::BadRequest("escribe el usuario y la contraseña".into()));
    }
    let salt = crypto::random_salt();
    let creds = Creds { u: username.clone(), t: crypto::subsonic_token(&req.password, &salt), s: salt };
    drop(req.password);

    let info = match st.subsonic.login(&creds).await {
        Ok(i) => i,
        Err(SubsonicError::BadCredentials) => {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            return Err(AppError::Unauthorized);
        }
        Err(e) => return Err(AppError::BadGateway(e.to_string())),
    };
    // Navidrome no distingue mayúsculas en el login: usamos el nombre que devuelve.
    let username = info.username;
    let t = now();

    let mut tx = st.db.begin().await?;
    let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE username = ? COLLATE NOCASE")
        .bind(&username)
        .fetch_optional(&mut *tx)
        .await?;
    let user_id = match existing {
        Some((id,)) => {
            sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?").bind(t).bind(id).execute(&mut *tx).await?;
            id
        }
        None => {
            let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(&mut *tx).await?;
            let first = count == 0;
            let approved = first || st.cfg.registration == "open";
            let (id,): (i64,) = sqlx::query_as(
                "INSERT INTO users (username, display_name, is_admin, is_approved, timezone, created_at, last_login_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(&username)
            .bind(&username)
            .bind(first)
            .bind(approved)
            .bind(&st.cfg.default_timezone)
            .bind(t)
            .bind(t)
            .fetch_one(&mut *tx)
            .await?;
            tracing::info!(usuario = %username, admin = first, aprobado = approved, "usuario nuevo");
            id
        }
    };
    let secret = st.crypto.encrypt(&serde_json::to_vec(&Creds { u: username.clone(), ..creds }).unwrap());
    sqlx::query(
        "INSERT INTO navidrome_credentials (user_id, secret_enc, status, updated_at) VALUES (?, ?, 'ok', ?) \
         ON CONFLICT(user_id) DO UPDATE SET secret_enc = excluded.secret_enc, status = 'ok', updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(&secret)
    .bind(t)
    .execute(&mut *tx)
    .await?;

    let sql = format!("SELECT {USER_COLS} FROM users u WHERE u.id = ?");
    let user: User = sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(user_id).fetch_one(&mut *tx).await?;
    if !user.is_approved {
        tx.commit().await?;
        return Err(AppError::Forbidden("tu cuenta está pendiente de que la apruebe un administrador".into()));
    }

    let token = crypto::random_token();
    let ua =
        headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(|s| s.chars().take(200).collect::<String>());
    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, created_at, expires_at, last_seen_at, user_agent) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(crypto::token_hash(&token))
    .bind(t)
    .bind(t + SESSION_DAYS * 86400)
    .bind(t)
    .bind(ua)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let mut resp = Json(LoginResp { user }).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, session_cookie(&st, &token, SESSION_DAYS * 86400));
    Ok(resp)
}

/// POST /api/v1/auth/logout
pub async fn logout(State(st): State<AppState>, headers: HeaderMap, _u: CurrentUser) -> AppResult<Response> {
    if let Some(tok) = cookie(&headers, SESSION_COOKIE) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?").bind(crypto::token_hash(&tok)).execute(&st.db).await?;
    }
    let mut resp = Json(serde_json::json!({"ok": true})).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, session_cookie(&st, "", 0));
    Ok(resp)
}

/// GET /api/v1/me
pub async fn me(CurrentUser(u): CurrentUser) -> Json<User> {
    Json(u)
}

#[derive(Deserialize)]
pub struct UpdateMe {
    pub display_name: Option<String>,
    pub timezone: Option<String>,
    pub is_public: Option<bool>,
}

/// PATCH /api/v1/me
pub async fn update_me(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Json(req): Json<UpdateMe>,
) -> AppResult<Json<User>> {
    if let Some(tz) = &req.timezone {
        tz.parse::<chrono_tz::Tz>().map_err(|_| AppError::BadRequest(format!("zona horaria desconocida: {tz}")))?;
    }
    if let Some(n) = &req.display_name {
        let n = n.trim();
        if n.is_empty() || n.chars().count() > 60 {
            return Err(AppError::BadRequest("el nombre debe tener entre 1 y 60 caracteres".into()));
        }
    }
    sqlx::query(
        "UPDATE users SET display_name = COALESCE(?, display_name), timezone = COALESCE(?, timezone), \
         is_public = COALESCE(?, is_public) WHERE id = ?",
    )
    .bind(req.display_name.as_deref().map(str::trim))
    .bind(&req.timezone)
    .bind(req.is_public)
    .bind(u.id)
    .execute(&st.db)
    .await?;
    let sql = format!("SELECT {USER_COLS} FROM users u WHERE u.id = ?");
    Ok(Json(sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(u.id).fetch_one(&st.db).await?))
}

/// Credenciales de Subsonic guardadas del usuario (descifradas).
pub async fn navidrome_creds(st: &AppState, user_id: i64) -> AppResult<Creds> {
    let row: Option<(Vec<u8>,)> =
        sqlx::query_as("SELECT secret_enc FROM navidrome_credentials WHERE user_id = ? AND status = 'ok'")
            .bind(user_id)
            .fetch_optional(&st.db)
            .await?;
    let (blob,) =
        row.ok_or_else(|| AppError::Forbidden("vuelve a iniciar sesión para reconectar con Navidrome".into()))?;
    let plain = st.crypto.decrypt(&blob)?;
    Ok(serde_json::from_slice(&plain).map_err(anyhow::Error::from)?)
}
