//! Conexión a SQLite (WAL) y migraciones.

use std::{path::Path, str::FromStr, time::Duration};

use anyhow::Result;
use sqlx::ConnectOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};

pub type Db = SqlitePool;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn open(path: &Path) -> Result<Db> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .log_slow_statements(log::LevelFilter::Warn, Duration::from_secs(5));
    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(opts).await?;
    MIGRATOR.run(&pool).await?;
    optimize(&pool).await;
    Ok(pool)
}

/// BD en memoria para los tests (una sola conexión, para que todos vean lo mismo).
pub async fn open_memory() -> Result<Db> {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// Actualiza las estadísticas del planificador de SQLite si las tablas han cambiado mucho.
/// Sin esto, tras una importación grande las consultas pueden ir 10 veces más lentas.
pub async fn optimize(db: &Db) {
    if let Err(e) = sqlx::query("PRAGMA optimize=0x10002").execute(db).await {
        tracing::warn!(error = %e, "PRAGMA optimize");
    }
}

/// ANALYZE completo (tras importaciones grandes).
pub async fn analyze(db: &Db) {
    if let Err(e) = sqlx::query("ANALYZE").execute(db).await {
        tracing::warn!(error = %e, "ANALYZE");
    }
}

/// Mantenimiento periódico de la BD.
pub async fn maintenance(db: Db) {
    loop {
        tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        optimize(&db).await;
    }
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
