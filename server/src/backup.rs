//! Copias de la BD con `VACUUM INTO` (consistentes aunque el servidor esté escribiendo).
//!
//! - Diarias (`diaria-*.db`) y semanales (`semanal-*.db`) en `<data_dir>/backups`, con rotación.
//! - Antes de aplicar una migración nueva (`pre-migracion-*.db`, ver `db::open`).
//! - A mano: `gordurasfm backup` (lo usa el actualizador antes de cambiar el binario).

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::{db::Db, state::AppState};

pub const DAILY: &str = "diaria";
pub const WEEKLY: &str = "semanal";
/// Copias a mano y antes de migrar: se guardan las 5 más nuevas de cada tipo.
pub const KEEP_OTHERS: usize = 5;

/// Hace una copia `<dir>/<prefix>-AAAAMMDD-HHMMSS.db` y devuelve su ruta.
pub async fn backup(db: &Db, dir: &Path, prefix: &str, now: DateTime<Utc>) -> Result<PathBuf> {
    tokio::fs::create_dir_all(dir).await.with_context(|| format!("creando {}", dir.display()))?;
    let path = dir.join(format!("{prefix}-{}.db", now.format("%Y%m%d-%H%M%S")));
    // VACUUM INTO falla si el destino existe.
    let _ = tokio::fs::remove_file(&path).await;
    let tmp = path.with_extension("db.tmp");
    let _ = tokio::fs::remove_file(&tmp).await;
    let quoted = tmp.to_string_lossy().replace('\'', "''");
    // La ruta la construimos nosotros (carpeta de datos + fecha), no viene del usuario.
    sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM INTO '{quoted}'")))
        .execute(db)
        .await
        .with_context(|| format!("VACUUM INTO {}", tmp.display()))?;
    // Solo el dueño (y su grupo) pueden leer la copia, aunque se haga desde una shell con umask 022.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o640)).await?;
        let _ = tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o750)).await;
    }
    tokio::fs::rename(&tmp, &path).await?;
    Ok(path)
}

/// Copias de un tipo, de la más nueva a la más vieja (el nombre lleva la fecha).
pub fn list(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                        n.starts_with(&format!("{prefix}-")) && n.ends_with(".db") && !n.ends_with(".tmp")
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.reverse();
    v
}

/// Fecha de una copia a partir de su nombre.
fn taken_at(p: &Path, prefix: &str) -> Option<DateTime<Utc>> {
    let n = p.file_name()?.to_str()?;
    let stamp = n.strip_prefix(&format!("{prefix}-"))?.strip_suffix(".db")?;
    chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok().map(|d| d.and_utc())
}

/// Borra las copias que sobran (deja las `keep` más nuevas). Devuelve las borradas.
pub fn rotate(dir: &Path, prefix: &str, keep: usize) -> Vec<PathBuf> {
    let mut removed = vec![];
    for p in list(dir, prefix).into_iter().skip(keep) {
        if std::fs::remove_file(&p).is_ok() {
            removed.push(p);
        }
    }
    removed
}

/// Una pasada del mantenimiento: copia diaria si la última tiene más de 23 h, semanal si la última
/// tiene más de 6 días y 20 h, y rotación.
pub async fn tick(db: &Db, dir: &Path, keep_daily: usize, keep_weekly: usize, now: DateTime<Utc>) -> Result<()> {
    let old = |prefix: &str, max: chrono::Duration| {
        list(dir, prefix).first().and_then(|p| taken_at(p, prefix)).is_none_or(|t| now - t > max)
    };
    if keep_daily > 0 && old(DAILY, chrono::Duration::hours(23)) {
        let p = backup(db, dir, DAILY, now).await?;
        tracing::info!(copia = %p.display(), "copia diaria de la BD");
    }
    if keep_weekly > 0 && old(WEEKLY, chrono::Duration::hours(6 * 24 + 20)) {
        let p = backup(db, dir, WEEKLY, now).await?;
        tracing::info!(copia = %p.display(), "copia semanal de la BD");
    }
    let others = rotate(dir, "manual", KEEP_OTHERS).into_iter().chain(rotate(dir, "pre-migracion", KEEP_OTHERS));
    for p in rotate(dir, DAILY, keep_daily).into_iter().chain(rotate(dir, WEEKLY, keep_weekly)).chain(others) {
        tracing::info!(copia = %p.display(), "copia antigua borrada");
    }
    Ok(())
}

/// Tarea de fondo: revisa cada hora (la primera, a los 5 minutos de arrancar).
pub async fn run(st: AppState) {
    let cfg = st.cfg.backup.clone();
    if !cfg.enabled {
        tracing::info!("copias automáticas de la BD desactivadas");
        return;
    }
    let dir = st.cfg.backups_dir();
    tokio::time::sleep(Duration::from_secs(300)).await;
    loop {
        if let Err(e) = tick(&st.db, &dir, cfg.keep_daily, cfg.keep_weekly, Utc::now()).await {
            tracing::error!(error = %format!("{e:#}"), "copia de la BD");
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn file_db(dir: &Path) -> Db {
        let db = crate::db::open(&dir.join("gordurasfm.db")).await.unwrap();
        sqlx::query("INSERT INTO users (username, display_name, created_at) VALUES ('ana', 'Ana', 0)")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[tokio::test]
    async fn copia_legible_y_rotacion() {
        let tmp = tempfile::tempdir().unwrap();
        let db = file_db(tmp.path()).await;
        let dir = tmp.path().join("backups");

        // Primera pasada: diaria y semanal.
        tick(&db, &dir, 2, 1, at("2026-11-01T04:00:00Z")).await.unwrap();
        assert_eq!(list(&dir, DAILY).len(), 1);
        assert_eq!(list(&dir, WEEKLY).len(), 1);
        // Una hora después no hace nada.
        tick(&db, &dir, 2, 1, at("2026-11-01T05:00:00Z")).await.unwrap();
        assert_eq!(list(&dir, DAILY).len(), 1);
        // Pasan tres días: se guardan solo las 2 diarias más nuevas; la semanal aún no toca.
        for d in ["2026-11-02T04:00:00Z", "2026-11-03T04:00:00Z", "2026-11-04T04:00:00Z"] {
            tick(&db, &dir, 2, 1, at(d)).await.unwrap();
        }
        let daily = list(&dir, DAILY);
        assert_eq!(daily.len(), 2);
        assert!(daily[0].to_string_lossy().contains("20261104"));
        assert!(daily[1].to_string_lossy().contains("20261103"));
        assert_eq!(list(&dir, WEEKLY).len(), 1);
        // Una semana después, semanal nueva y la vieja se borra (keep_weekly = 1).
        tick(&db, &dir, 2, 1, at("2026-11-08T04:00:00Z")).await.unwrap();
        let weekly = list(&dir, WEEKLY);
        assert_eq!(weekly.len(), 1);
        assert!(weekly[0].to_string_lossy().contains("20261108"));

        // Las copias a mano se quedan en las 5 más nuevas.
        for m in 0..7 {
            backup(&db, &dir, "manual", at("2026-11-08T05:00:00Z") + chrono::Duration::minutes(m)).await.unwrap();
        }
        tick(&db, &dir, 2, 1, at("2026-11-08T06:00:00Z")).await.unwrap();
        assert_eq!(list(&dir, "manual").len(), KEEP_OTHERS);
        assert_eq!(list(&dir, DAILY).len(), 2, "las diarias no se tocan");

        // La copia es una BD válida con los datos.
        let copy = crate::db::open(&daily[0]).await.unwrap();
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(&copy).await.unwrap();
        assert_eq!(n, 1);
        assert!(std::fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().path().to_string_lossy().ends_with(".tmp")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&daily[0]).unwrap().permissions().mode() & 0o777, 0o640);
            assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o750);
        }
    }
}
