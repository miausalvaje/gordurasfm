//! Ajustes de la instancia guardados en la BD (los edita un admin).

use crate::{crypto::Crypto, db::Db};

pub async fn get(db: &Db, key: &str) -> sqlx::Result<Option<String>> {
    Ok(sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await?
        .map(|r| r.0))
}

pub async fn set(db: &Db, key: &str, value: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn delete(db: &Db, key: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM settings WHERE key = ?").bind(key).execute(db).await?;
    Ok(())
}

/// Guarda un secreto cifrado (hex).
pub async fn set_secret(db: &Db, crypto: &Crypto, key: &str, value: &str) -> sqlx::Result<()> {
    set(db, key, &hex::encode(crypto.encrypt_str(value))).await
}

pub async fn get_secret(db: &Db, crypto: &Crypto, key: &str) -> anyhow::Result<Option<String>> {
    match get(db, key).await? {
        Some(h) => Ok(Some(crypto.decrypt_str(&hex::decode(h)?)?)),
        None => Ok(None),
    }
}
