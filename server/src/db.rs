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
    let existed = std::fs::metadata(path).is_ok_and(|m| m.len() > 0);
    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(opts).await?;
    if existed {
        backup_before_migrations(&pool, path).await?;
    }
    MIGRATOR.run(&pool).await?;
    optimize(&pool).await;
    Ok(pool)
}

/// BD existente, solo lectura y sin migrar (para `gordurasfm backup` con el servidor en marcha).
pub async fn open_read_only(path: &Path) -> Result<Db> {
    anyhow::ensure!(path.exists(), "no existe la BD {}", path.display());
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .read_only(true)
        .busy_timeout(Duration::from_secs(30));
    Ok(SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?)
}

/// Versiones de migración que faltan por aplicar en esta BD.
pub async fn pending_migrations(db: &Db) -> Result<Vec<i64>> {
    let (has_table,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'")
            .fetch_one(db)
            .await?;
    let applied: Vec<i64> = if has_table > 0 {
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1").fetch_all(db).await?
    } else {
        vec![]
    };
    Ok(MIGRATOR.iter().map(|m| m.version).filter(|v| !applied.contains(v)).collect())
}

/// Si hay migraciones nuevas, copia la BD antes de aplicarlas (`backups/pre-migracion-*.db`).
async fn backup_before_migrations(db: &Db, path: &Path) -> Result<()> {
    let pending = pending_migrations(db).await?;
    if pending.is_empty() {
        return Ok(());
    }
    let dir = path.parent().unwrap_or(Path::new(".")).join("backups");
    let copy = crate::backup::backup(db, &dir, "pre-migracion", chrono::Utc::now()).await?;
    tracing::info!(copia = %copy.display(), migraciones = ?pending, "copia de la BD antes de migrar");
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn copia_antes_de_migrar() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("gordurasfm.db");
        let db = open(&path).await.unwrap();
        assert!(pending_migrations(&db).await.unwrap().is_empty());
        // Sin migraciones pendientes no se copia nada.
        backup_before_migrations(&db, &path).await.unwrap();
        assert!(!tmp.path().join("backups").exists());
        // Se simula una BD de una versión anterior.
        let last = MIGRATOR.iter().map(|m| m.version).max().unwrap();
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?").bind(last).execute(&db).await.unwrap();
        assert_eq!(pending_migrations(&db).await.unwrap(), vec![last]);
        backup_before_migrations(&db, &path).await.unwrap();
        assert_eq!(crate::backup::list(&tmp.path().join("backups"), "pre-migracion").len(), 1);
        // La copia de solo lectura funciona con la BD abierta.
        let ro = open_read_only(&path).await.unwrap();
        let out = crate::backup::backup(&ro, &tmp.path().join("b2"), "manual", chrono::Utc::now()).await.unwrap();
        assert!(out.exists());
    }
}
