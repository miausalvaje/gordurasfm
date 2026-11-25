//! Administración: key de Last.fm de la instancia y aprobación de usuarios.

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    lastfm::{
        self,
        client::{Key, LfmError},
    },
    settings,
    state::AppState,
};

fn require_admin(u: &crate::auth::User) -> AppResult<()> {
    if u.is_admin { Ok(()) } else { Err(AppError::Forbidden("solo para administradores".into())) }
}

/// GET /api/v1/admin/lastfm
pub async fn get_lastfm(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Value>> {
    require_admin(&u)?;
    let from_config = st.cfg.lastfm.api_key.is_some() && st.cfg.lastfm.api_secret.is_some();
    let key = lastfm::instance_key(&st).await?;
    Ok(Json(json!({
        "configured": key.is_some(),
        "source": if from_config { "config" } else if key.is_some() { "settings" } else { "none" },
        "api_key": key.map(|k| k.api_key),
    })))
}

#[derive(Deserialize)]
pub struct LastfmKeyReq {
    pub api_key: String,
    pub api_secret: String,
}

/// PUT /api/v1/admin/lastfm — guarda la key de la instancia (el secret, cifrado).
pub async fn put_lastfm(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Json(req): Json<LastfmKeyReq>,
) -> AppResult<Json<Value>> {
    require_admin(&u)?;
    let (k, s) = (req.api_key.trim(), req.api_secret.trim());
    if k.len() != 32 || s.len() != 32 || !k.chars().chain(s.chars()).all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest("la key y el secret de Last.fm son 32 caracteres hexadecimales".into()));
    }
    match st.lastfm.user_exists(&Key { api_key: k.into(), secret: s.into() }, "rj").await {
        Ok(_) | Err(LfmError::NotFound(_)) => {}
        Err(LfmError::Auth(m)) => return Err(AppError::BadRequest(format!("Last.fm no acepta esa key: {m}"))),
        Err(e) => return Err(AppError::BadGateway(e.to_string())),
    }
    settings::set(&st.db, "lastfm_api_key", k).await?;
    settings::set_secret(&st.db, &st.crypto, "lastfm_api_secret", s).await?;
    Ok(Json(json!({"ok": true})))
}

/// DELETE /api/v1/admin/lastfm
pub async fn delete_lastfm(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Value>> {
    require_admin(&u)?;
    settings::delete(&st.db, "lastfm_api_key").await?;
    settings::delete(&st.db, "lastfm_api_secret").await?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Serialize, FromRow)]
pub struct AdminUser {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
    pub is_approved: bool,
    pub created_at: i64,
    pub last_login_at: Option<i64>,
    pub scrobbles: i64,
}

/// GET /api/v1/admin/users
pub async fn users(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Vec<AdminUser>>> {
    require_admin(&u)?;
    Ok(Json(
        sqlx::query_as(
            "SELECT u.id, u.username, u.is_admin, u.is_approved, u.created_at, u.last_login_at, \
                    (SELECT COUNT(*) FROM scrobbles s WHERE s.user_id = u.id) AS scrobbles FROM users u ORDER BY u.id",
        )
        .fetch_all(&st.db)
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct UserPatch {
    pub is_approved: Option<bool>,
    pub is_admin: Option<bool>,
}

/// PATCH /api/v1/admin/users/{id}
pub async fn patch_user(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(id): Path<i64>,
    Json(req): Json<UserPatch>,
) -> AppResult<Json<Value>> {
    require_admin(&u)?;
    if id == u.id && (req.is_admin == Some(false) || req.is_approved == Some(false)) {
        return Err(AppError::BadRequest("no puedes quitarte a ti mismo el admin ni la aprobación".into()));
    }
    let r = sqlx::query(
        "UPDATE users SET is_approved = COALESCE(?, is_approved), is_admin = COALESCE(?, is_admin) WHERE id = ?",
    )
    .bind(req.is_approved)
    .bind(req.is_admin)
    .bind(id)
    .execute(&st.db)
    .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if req.is_approved == Some(false) {
        sqlx::query("DELETE FROM sessions WHERE user_id = ?").bind(id).execute(&st.db).await?;
    }
    Ok(Json(json!({"ok": true})))
}
