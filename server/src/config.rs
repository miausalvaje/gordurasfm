//! Configuración: `config.toml` + variables de entorno `OWNFM_*`.

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Dirección de escucha, p. ej. `0.0.0.0:8090`.
    pub listen: SocketAddr,
    /// Carpeta de datos (BD, imágenes, copias).
    pub data_dir: PathBuf,
    /// Archivo con la clave maestra (32 bytes en hex). Se crea si no existe.
    pub master_key_file: Option<PathBuf>,
    /// URL pública con la que se llega a OwnFM (para cookies y callbacks).
    pub public_url: String,
    /// `auto` (según `public_url`), `true` o `false`.
    pub cookie_secure: String,
    pub default_timezone: String,
    /// `open` (cualquier usuario de Navidrome) o `approval` (lo aprueba un admin).
    pub registration: String,
    pub navidrome: NavidromeConfig,
    pub listenbrainz: ListenBrainzConfig,
    pub lastfm: LastfmConfig,
    pub forward: ForwardConfig,
    pub enrich: EnrichConfig,
    pub backup: BackupConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    /// Copias automáticas de la BD en `<data_dir>/backups`.
    pub enabled: bool,
    pub keep_daily: usize,
    pub keep_weekly: usize,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self { enabled: true, keep_daily: 7, keep_weekly: 4 }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LastfmConfig {
    pub api_url: String,
    pub auth_url: String,
    /// Key y secret de la instancia. También se pueden poner desde la web (admin).
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    /// Pausa entre páginas al importar (ms). Last.fm pide no pasar de 5 peticiones/s.
    pub import_delay_ms: u64,
}

impl Default for LastfmConfig {
    fn default() -> Self {
        Self {
            api_url: "https://ws.audioscrobbler.com/2.0/".into(),
            auth_url: "https://www.last.fm/api/auth/".into(),
            api_key: None,
            api_secret: None,
            import_delay_ms: 250,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EnrichConfig {
    /// Buscar portadas e ids en Navidrome.
    pub enabled: bool,
    pub interval_secs: u64,
    /// Pausa entre búsquedas en Navidrome (ms).
    pub delay_ms: u64,
}

impl Default for EnrichConfig {
    fn default() -> Self {
        Self { enabled: true, interval_secs: 60, delay_ms: 100 }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NavidromeConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ListenBrainzConfig {
    /// Raíz de la API real de ListenBrainz (sin `/1/`).
    pub api_url: String,
    /// Reenviar tal cual a ListenBrainz los tokens que no son de nadie.
    pub passthrough_unknown_tokens: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ForwardConfig {
    /// Espera del primer reintento (segundos). Se dobla en cada intento.
    pub base_delay_secs: i64,
    pub max_delay_secs: i64,
    /// Cada cuánto se revisa la cola aunque no haya avisos (segundos).
    pub poll_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:8090".parse().unwrap(),
            data_dir: PathBuf::from("./data"),
            master_key_file: None,
            public_url: "http://localhost:8090".into(),
            cookie_secure: "auto".into(),
            default_timezone: "Europe/Madrid".into(),
            registration: "open".into(),
            navidrome: NavidromeConfig::default(),
            listenbrainz: ListenBrainzConfig::default(),
            lastfm: LastfmConfig::default(),
            forward: ForwardConfig::default(),
            enrich: EnrichConfig::default(),
            backup: BackupConfig::default(),
        }
    }
}

impl Default for NavidromeConfig {
    fn default() -> Self {
        Self { url: "http://localhost:4533".into() }
    }
}

impl Default for ListenBrainzConfig {
    fn default() -> Self {
        Self { api_url: "https://api.listenbrainz.org".into(), passthrough_unknown_tokens: true }
    }
}

impl Default for ForwardConfig {
    fn default() -> Self {
        Self { base_delay_secs: 30, max_delay_secs: 6 * 3600, poll_secs: 15 }
    }
}

/// Valor de la variable con el prefijo antiguo (`GFM_…` en vez de `OWNFM_…`), avisando en el registro.
pub fn legacy_env(k: &str, get: &impl Fn(&str) -> Option<String>) -> Option<String> {
    let old = format!("GFM_{}", k.strip_prefix("OWNFM_")?);
    let v = get(&old)?;
    tracing::warn!(variable = %old, nueva = k, "variable con el nombre antiguo; renómbrala");
    Some(v)
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self> {
        let mut cfg = match path {
            Some(p) => {
                let txt = std::fs::read_to_string(p).with_context(|| format!("leyendo {p}"))?;
                toml::from_str(&txt).with_context(|| format!("{p} no es TOML válido"))?
            }
            None => Config::default(),
        };
        cfg.apply_env(|k| std::env::var(k).ok())?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Aplica las variables `OWNFM_*`. Por compatibilidad con GordurasFM (nombre anterior del proyecto),
    /// si una no está se mira la misma con el prefijo `GFM_`.
    pub fn apply_env(&mut self, get: impl Fn(&str) -> Option<String>) -> Result<()> {
        let get = |k: &str| get(k).or_else(|| legacy_env(k, &get));
        if let Some(v) = get("OWNFM_LISTEN") {
            self.listen = v.parse().context("OWNFM_LISTEN")?;
        }
        if let Some(v) = get("OWNFM_DATA_DIR") {
            self.data_dir = v.into();
        }
        if let Some(v) = get("OWNFM_MASTER_KEY_FILE") {
            self.master_key_file = Some(v.into());
        }
        if let Some(v) = get("OWNFM_PUBLIC_URL") {
            self.public_url = v;
        }
        if let Some(v) = get("OWNFM_COOKIE_SECURE") {
            self.cookie_secure = v;
        }
        if let Some(v) = get("OWNFM_DEFAULT_TIMEZONE") {
            self.default_timezone = v;
        }
        if let Some(v) = get("OWNFM_REGISTRATION") {
            self.registration = v;
        }
        if let Some(v) = get("OWNFM_NAVIDROME_URL") {
            self.navidrome.url = v;
        }
        if let Some(v) = get("OWNFM_LISTENBRAINZ_API_URL") {
            self.listenbrainz.api_url = v;
        }
        if let Some(v) = get("OWNFM_LASTFM_API_URL") {
            self.lastfm.api_url = v;
        }
        if let Some(v) = get("OWNFM_LASTFM_API_KEY") {
            self.lastfm.api_key = Some(v);
        }
        if let Some(v) = get("OWNFM_LASTFM_API_SECRET") {
            self.lastfm.api_secret = Some(v);
        }
        Ok(())
    }

    pub fn validate(&mut self) -> Result<()> {
        self.navidrome.url = self.navidrome.url.trim_end_matches('/').to_string();
        self.listenbrainz.api_url = self.listenbrainz.api_url.trim_end_matches('/').to_string();
        self.public_url = self.public_url.trim_end_matches('/').to_string();
        self.default_timezone
            .parse::<chrono_tz::Tz>()
            .map_err(|_| anyhow::anyhow!("zona horaria desconocida: {}", self.default_timezone))?;
        anyhow::ensure!(
            matches!(self.registration.as_str(), "open" | "approval"),
            "registration debe ser «open» o «approval»"
        );
        Ok(())
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("ownfm.db")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.data_dir.join("images")
    }

    pub fn imports_dir(&self) -> PathBuf {
        self.data_dir.join("imports")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    pub fn key_path(&self) -> PathBuf {
        self.master_key_file.clone().unwrap_or_else(|| self.data_dir.join("master.key"))
    }

    pub fn secure_cookies(&self) -> bool {
        match self.cookie_secure.as_str() {
            "true" => true,
            "false" => false,
            _ => self.public_url.starts_with("https://"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_ejemplo_es_valido_y_coincide_con_los_valores_por_defecto() {
        let mut c: Config = toml::from_str(include_str!("../config.example.toml")).unwrap();
        c.validate().unwrap();
        let d = Config::default();
        assert_eq!(c.listen, d.listen);
        assert_eq!(c.forward.base_delay_secs, d.forward.base_delay_secs);
        assert_eq!(c.listenbrainz.api_url, d.listenbrainz.api_url);
        assert_eq!(c.backup.keep_daily, d.backup.keep_daily);
        assert_eq!(c.backup.keep_weekly, d.backup.keep_weekly);
    }

    #[test]
    fn variables_de_entorno() {
        let mut c = Config::default();
        c.apply_env(|k| match k {
            "OWNFM_NAVIDROME_URL" => Some("http://nd:4533/".into()),
            "OWNFM_REGISTRATION" => Some("approval".into()),
            _ => None,
        })
        .unwrap();
        c.validate().unwrap();
        assert_eq!(c.navidrome.url, "http://nd:4533");
        assert_eq!(c.registration, "approval");
        c.registration = "x".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn variables_con_el_nombre_antiguo() {
        let mut c = Config::default();
        c.apply_env(|k| match k {
            "GFM_NAVIDROME_URL" => Some("http://viejo:4533".into()),
            "GFM_PUBLIC_URL" => Some("http://viejo".into()),
            "OWNFM_PUBLIC_URL" => Some("http://nuevo".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(c.navidrome.url, "http://viejo:4533", "GFM_ sirve si no hay OWNFM_");
        assert_eq!(c.public_url, "http://nuevo", "OWNFM_ manda sobre GFM_");
    }
}
