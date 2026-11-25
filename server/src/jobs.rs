//! Trabajos en segundo plano: importar de Last.fm (reanudable) y de ficheros.
//!
//! El estado vive en la tabla `jobs` (cursor + progreso), así que si el servicio se reinicia,
//! el trabajo sigue por donde iba.

use std::time::Duration;

use axum::{
    Json,
    extract::{Multipart, Path, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;

use crate::{
    auth::CurrentUser,
    db::now,
    error::{AppError, AppResult},
    lastfm::{self, client::LfmError},
    scrobble::{self, Outcome},
    state::AppState,
};

const PAGE_SIZE: i64 = 200;
const MAX_RETRIES: u32 = 8;
const FILE_CHUNK: usize = 500;
pub const MAX_UPLOAD: usize = 200 * 1024 * 1024;

#[derive(Debug, Serialize, FromRow)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub status: String,
    pub progress_done: i64,
    pub progress_total: Option<i64>,
    #[serde(skip)]
    pub cursor: Option<String>,
    #[sqlx(json(nullable))]
    pub stats: Option<Value>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip)]
    pub user_id: Option<i64>,
}

const JOB_COLS: &str =
    "id, kind, status, progress_done, progress_total, cursor, stats, error, created_at, updated_at, user_id";

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ImportStats {
    pub stored: i64,
    pub duplicates: i64,
    pub ignored: i64,
    pub invalid: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct LastfmCursor {
    user: String,
    /// Fijo al empezar: así las escuchas nuevas no desplazan las páginas.
    to: i64,
    from: Option<i64>,
    /// Siguiente página a pedir (1 = la más reciente).
    page: i64,
    total_pages: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FileCursor {
    path: String,
    name: String,
    /// Escuchas ya procesadas.
    done: usize,
}

/// Bucle del gestor: procesa los trabajos en cola o a medias, uno detrás de otro.
pub async fn run(st: AppState) {
    tracing::info!("gestor de trabajos en marcha");
    loop {
        loop {
            let sql = format!("SELECT {JOB_COLS} FROM jobs WHERE status IN ('queued', 'running') ORDER BY id LIMIT 1");
            let job: Option<Job> = match sqlx::query_as(sqlx::AssertSqlSafe(sql)).fetch_optional(&st.db).await {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!(error = %e, "trabajos");
                    break;
                }
            };
            let Some(job) = job else { break };
            if let Err(e) = run_job(&st, &job).await {
                tracing::error!(trabajo = job.id, error = %e, "trabajo fallido");
                let _ = finish(&st, job.id, "failed", Some(&e.to_string())).await;
            }
        }
        tokio::select! {
            _ = st.jobs_wake.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(30)) => {}
        }
    }
}

async fn finish(st: &AppState, id: i64, status: &str, error: Option<&str>) -> sqlx::Result<()> {
    sqlx::query("UPDATE jobs SET status = ?, error = ?, updated_at = ? WHERE id = ?")
        .bind(status)
        .bind(error)
        .bind(now())
        .bind(id)
        .execute(&st.db)
        .await?;
    // Tras meter muchas escuchas, el planificador de SQLite necesita estadísticas nuevas.
    crate::db::analyze(&st.db).await;
    Ok(())
}

async fn save(
    st: &AppState,
    id: i64,
    cursor: &impl Serialize,
    done: i64,
    total: Option<i64>,
    stats: &ImportStats,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE jobs SET status = 'running', cursor = ?, progress_done = ?, progress_total = ?, stats = ?, error = NULL, updated_at = ? WHERE id = ?")
        .bind(serde_json::to_string(cursor).unwrap())
        .bind(done)
        .bind(total)
        .bind(serde_json::to_string(stats).unwrap())
        .bind(now())
        .bind(id)
        .execute(&st.db)
        .await?;
    Ok(())
}

/// ¿Lo han cancelado mientras tanto?
async fn cancelled(st: &AppState, id: i64) -> bool {
    matches!(
        sqlx::query_as::<_, (String,)>("SELECT status FROM jobs WHERE id = ?").bind(id).fetch_optional(&st.db).await,
        Ok(Some((s,))) if s == "cancelled"
    )
}

async fn run_job(st: &AppState, job: &Job) -> anyhow::Result<()> {
    let user_id = job.user_id.ok_or_else(|| anyhow::anyhow!("trabajo sin usuario"))?;
    let stats: ImportStats = job.stats.clone().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    match job.kind.as_str() {
        "lastfm_import" => run_lastfm(st, job, user_id, stats).await,
        "file_import" => run_file(st, job, user_id, stats).await,
        k => anyhow::bail!("tipo de trabajo desconocido: {k}"),
    }
}

async fn ingest_all(
    st: &AppState,
    user_id: i64,
    listens: &[scrobble::Listen],
    stats: &mut ImportStats,
) -> sqlx::Result<()> {
    let _turno = st.write_lock.lock().await;
    for o in scrobble::ingest_many(&st.db, user_id, listens).await? {
        match o {
            Outcome::Stored(_) => stats.stored += 1,
            Outcome::Duplicate => stats.duplicates += 1,
            Outcome::Ignored(_) => stats.ignored += 1,
        }
    }
    Ok(())
}

async fn run_lastfm(st: &AppState, job: &Job, user_id: i64, mut stats: ImportStats) -> anyhow::Result<()> {
    let mut cur: LastfmCursor = serde_json::from_str(job.cursor.as_deref().unwrap_or("{}"))?;
    let key =
        lastfm::key_for(st, user_id).await?.ok_or_else(|| anyhow::anyhow!("falta configurar la API key de Last.fm"))?;
    let delay = Duration::from_millis(st.cfg.lastfm.import_delay_ms);
    let mut retries = 0u32;
    tracing::info!(trabajo = job.id, usuario = user_id, lastfm = %cur.user, pagina = cur.page, "importando de Last.fm");
    loop {
        if cancelled(st, job.id).await {
            return Ok(());
        }
        if let Some(tp) = cur.total_pages
            && cur.page > tp
        {
            break;
        }
        match st.lastfm.recent_tracks(&key, &cur.user, cur.page, PAGE_SIZE, cur.from, Some(cur.to)).await {
            Ok(page) => {
                retries = 0;
                cur.total_pages = Some(page.total_pages);
                let listens: Vec<_> = page.tracks.iter().filter_map(lastfm::listen_from_recent).collect();
                stats.invalid += (page.tracks.len() - listens.len()) as i64;
                ingest_all(st, user_id, &listens, &mut stats).await?;
                let done = cur.page;
                cur.page += 1;
                save(st, job.id, &cur, done.min(page.total_pages), Some(page.total_pages), &stats).await?;
                st.enrich_wake.notify_one();
                if page.tracks.is_empty() || cur.page > page.total_pages {
                    break;
                }
            }
            Err(LfmError::Retry(m)) if retries < MAX_RETRIES => {
                retries += 1;
                let wait = Duration::from_secs(2u64.pow(retries).min(120));
                tracing::warn!(trabajo = job.id, error = %m, ?wait, "Last.fm falló; reintento");
                tokio::time::sleep(wait).await;
                continue;
            }
            Err(e) => {
                // Se queda «failed» con el cursor guardado: se puede reanudar.
                finish(st, job.id, "failed", Some(&e.to_string())).await?;
                return Ok(());
            }
        }
        tokio::time::sleep(delay).await;
    }
    finish(st, job.id, "done", None).await?;
    tracing::info!(
        trabajo = job.id,
        guardadas = stats.stored,
        duplicadas = stats.duplicates,
        "importación de Last.fm terminada"
    );
    Ok(())
}

async fn run_file(st: &AppState, job: &Job, user_id: i64, mut stats: ImportStats) -> anyhow::Result<()> {
    let mut cur: FileCursor = serde_json::from_str(job.cursor.as_deref().unwrap_or("{}"))?;
    let text = tokio::fs::read_to_string(&cur.path)
        .await
        .map_err(|e| anyhow::anyhow!("no se puede leer el fichero subido: {e}"))?;
    let (format, it) = crate::import_file::records(&text).map_err(|e| anyhow::anyhow!(e))?;
    let total = job.progress_total;
    let mut it = it.flatten().skip(cur.done);
    loop {
        if cancelled(st, job.id).await {
            return Ok(());
        }
        let chunk: Vec<scrobble::Listen> = it.by_ref().take(FILE_CHUNK).collect();
        if chunk.is_empty() {
            break;
        }
        ingest_all(st, user_id, &chunk, &mut stats).await?;
        cur.done += chunk.len();
        save(st, job.id, &cur, cur.done as i64, total, &stats).await?;
        st.enrich_wake.notify_one();
    }
    drop(it);
    finish(st, job.id, "done", None).await?;
    let _ = tokio::fs::remove_file(&cur.path).await;
    tracing::info!(trabajo = job.id, formato = format, guardadas = stats.stored, "importación de fichero terminada");
    Ok(())
}

// ---------- API ----------

async fn active_job(st: &AppState, user_id: i64) -> sqlx::Result<Option<i64>> {
    Ok(sqlx::query_as::<_, (i64,)>("SELECT id FROM jobs WHERE user_id = ? AND status IN ('queued', 'running') LIMIT 1")
        .bind(user_id)
        .fetch_optional(&st.db)
        .await?
        .map(|r| r.0))
}

async fn create(
    st: &AppState,
    user_id: i64,
    kind: &str,
    cursor: &impl Serialize,
    total: Option<i64>,
) -> AppResult<Job> {
    if active_job(st, user_id).await?.is_some() {
        return Err(AppError::BadRequest("ya tienes una importación en marcha".into()));
    }
    let t = now();
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO jobs (user_id, kind, status, cursor, progress_done, progress_total, stats, created_at, updated_at) \
         VALUES (?, ?, 'queued', ?, 0, ?, ?, ?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(kind)
    .bind(serde_json::to_string(cursor).unwrap())
    .bind(total)
    .bind(serde_json::to_string(&ImportStats::default()).unwrap())
    .bind(t)
    .bind(t)
    .fetch_one(&st.db)
    .await?;
    st.jobs_wake.notify_one();
    get_job(st, user_id, id).await
}

async fn get_job(st: &AppState, user_id: i64, id: i64) -> AppResult<Job> {
    let sql = format!("SELECT {JOB_COLS} FROM jobs WHERE id = ? AND user_id = ?");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(user_id)
        .fetch_optional(&st.db)
        .await?
        .ok_or(AppError::NotFound)
}

#[derive(Deserialize)]
pub struct LastfmImportReq {
    /// `full` (todo el historial) o `new` (solo lo nuevo desde la última importación).
    pub mode: Option<String>,
    /// Usuario de Last.fm. Por defecto, el de la cuenta conectada.
    pub username: Option<String>,
}

/// POST /api/v1/import/lastfm
pub async fn start_lastfm(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Json(req): Json<LastfmImportReq>,
) -> AppResult<Json<Job>> {
    let key = lastfm::key_for(&st, u.id)
        .await?
        .ok_or_else(|| AppError::BadRequest("falta configurar la API key de Last.fm (admin o tu propia key)".into()))?;
    let connected: Option<(Option<String>,)> =
        sqlx::query_as("SELECT remote_username FROM connections WHERE user_id = ? AND provider = 'lastfm'")
            .bind(u.id)
            .fetch_optional(&st.db)
            .await?;
    let user = match req.username.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => n.to_string(),
        None => connected
            .and_then(|c| c.0)
            .ok_or_else(|| AppError::BadRequest("conecta tu cuenta de Last.fm o indica el usuario".into()))?,
    };
    let user = match st.lastfm.user_exists(&key, &user).await {
        Ok(n) => n,
        Err(LfmError::NotFound(_)) => {
            return Err(AppError::BadRequest(format!("Last.fm no conoce al usuario «{user}»")));
        }
        Err(e) => return Err(AppError::BadGateway(e.to_string())),
    };
    let from = match req.mode.as_deref().unwrap_or("full") {
        "full" => None,
        "new" => {
            let (last,): (Option<i64>,) =
                sqlx::query_as("SELECT MAX(played_at) FROM scrobbles WHERE user_id = ? AND source = 'lastfm'")
                    .bind(u.id)
                    .fetch_one(&st.db)
                    .await?;
            Some(last.map(|t| t - 3600).unwrap_or(0))
        }
        m => return Err(AppError::BadRequest(format!("modo desconocido: {m} (full o new)"))),
    };
    let cur = LastfmCursor { user, to: now() + 60, from, page: 1, total_pages: None };
    Ok(Json(create(&st, u.id, "lastfm_import", &cur, None).await?))
}

/// POST /api/v1/import/file (multipart, campo `file`). El fichero se escribe en disco a trozos.
pub async fn upload_file(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    mut mp: Multipart,
) -> AppResult<Json<Value>> {
    use tokio::io::AsyncWriteExt;
    if active_job(&st, u.id).await?.is_some() {
        return Err(AppError::BadRequest("ya tienes una importación en marcha".into()));
    }
    let bad = |e: axum::extract::multipart::MultipartError| AppError::BadRequest(format!("subida no válida: {e}"));
    while let Some(mut field) = mp.next_field().await.map_err(bad)? {
        if field.name() != Some("file") {
            continue;
        }
        let name = field.file_name().unwrap_or("historial").chars().take(120).collect::<String>();
        let dir = st.cfg.imports_dir();
        tokio::fs::create_dir_all(&dir).await.map_err(anyhow::Error::from)?;
        let path = dir.join(format!("{}-{}.dat", u.id, crate::crypto::random_token()));
        let mut f = tokio::fs::File::create(&path).await.map_err(anyhow::Error::from)?;
        while let Some(chunk) = field.chunk().await.map_err(bad)? {
            f.write_all(&chunk).await.map_err(anyhow::Error::from)?;
        }
        f.flush().await.map_err(anyhow::Error::from)?;
        drop(f);
        // Se comprueba ya, para avisar enseguida si el formato no vale.
        let checked = async {
            let text = tokio::fs::read_to_string(&path)
                .await
                .map_err(|_| AppError::BadRequest("el fichero debe estar en UTF-8".into()))?;
            let (format, ok, invalid) = crate::import_file::summarize(&text).map_err(AppError::BadRequest)?;
            if ok == 0 {
                return Err(AppError::BadRequest(format!(
                    "no se ha encontrado ninguna escucha válida ({invalid} registros sin leer)"
                )));
            }
            Ok((format, ok, invalid))
        }
        .await;
        let (format, ok, invalid) = match checked {
            Ok(v) => v,
            Err(e) => {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(e);
            }
        };
        let cur = FileCursor { path: path.display().to_string(), name, done: 0 };
        let job = create(&st, u.id, "file_import", &cur, Some(ok as i64)).await?;
        sqlx::query("UPDATE jobs SET stats = ? WHERE id = ?")
            .bind(serde_json::to_string(&ImportStats { invalid: invalid as i64, ..Default::default() }).unwrap())
            .bind(job.id)
            .execute(&st.db)
            .await?;
        return Ok(Json(json!({"job": job, "format": format, "listens": ok, "invalid": invalid})));
    }
    Err(AppError::BadRequest("falta el campo «file»".into()))
}

/// GET /api/v1/import/jobs
pub async fn list(State(st): State<AppState>, CurrentUser(u): CurrentUser) -> AppResult<Json<Vec<Job>>> {
    let sql = format!("SELECT {JOB_COLS} FROM jobs WHERE user_id = ? ORDER BY id DESC LIMIT 50");
    Ok(Json(sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(u.id).fetch_all(&st.db).await?))
}

/// GET /api/v1/import/jobs/{id}
pub async fn get(State(st): State<AppState>, CurrentUser(u): CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Job>> {
    Ok(Json(get_job(&st, u.id, id).await?))
}

/// POST /api/v1/import/jobs/{id}/cancel
pub async fn cancel(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Job>> {
    let r = sqlx::query("UPDATE jobs SET status = 'cancelled', updated_at = ? WHERE id = ? AND user_id = ? AND status IN ('queued', 'running', 'failed')")
        .bind(now())
        .bind(id)
        .bind(u.id)
        .execute(&st.db)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::BadRequest("ese trabajo ya ha terminado".into()));
    }
    Ok(Json(get_job(&st, u.id, id).await?))
}

/// POST /api/v1/import/jobs/{id}/resume — sigue un trabajo fallido o cancelado por donde iba.
pub async fn resume(
    State(st): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Job>> {
    if active_job(&st, u.id).await?.is_some() {
        return Err(AppError::BadRequest("ya tienes una importación en marcha".into()));
    }
    let r = sqlx::query("UPDATE jobs SET status = 'queued', error = NULL, updated_at = ? WHERE id = ? AND user_id = ? AND status IN ('failed', 'cancelled')")
        .bind(now())
        .bind(id)
        .bind(u.id)
        .execute(&st.db)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::BadRequest("solo se pueden reanudar trabajos fallidos o cancelados".into()));
    }
    st.jobs_wake.notify_one();
    Ok(Json(get_job(&st, u.id, id).await?))
}
