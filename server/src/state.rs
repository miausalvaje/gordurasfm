//! Estado compartido de la aplicación.

use std::{collections::HashMap, sync::Arc};

use serde::Serialize;
use tokio::sync::{Notify, RwLock};

use crate::{
    config::Config, crypto::Crypto, db::Db, lastfm::client::LastFm, listenbrainz::client::ListenBrainz,
    stats::StatsCache, subsonic::Subsonic,
};

#[derive(Debug, Clone, Serialize)]
pub struct NowPlaying {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub client: Option<String>,
    /// Momento (UTC, s) en que se recibió.
    pub since: i64,
}

pub struct Inner {
    pub cfg: Config,
    pub db: Db,
    pub crypto: Crypto,
    pub http: reqwest::Client,
    pub subsonic: Subsonic,
    pub lb: ListenBrainz,
    pub lastfm: LastFm,
    pub stats_cache: StatsCache,
    /// Peticiones de conexión con Last.fm en curso: state → (usuario, momento).
    pub lastfm_pending: std::sync::Mutex<HashMap<String, (i64, std::time::Instant)>>,
    pub now_playing: RwLock<HashMap<i64, NowPlaying>>,
    /// Despierta al reenviador cuando entra algo en la cola.
    pub forward_wake: Notify,
    /// Despierta al gestor de importaciones.
    pub jobs_wake: Notify,
    /// Despierta la sincronización periódica con Last.fm (al activarla, para no esperar al siguiente ciclo).
    pub lastfm_sync_wake: Notify,
    /// Despierta al enriquecedor (portadas e ids de Navidrome).
    pub enrich_wake: Notify,
    /// Turno de escritura (cola justa): importaciones, escuchas en vivo y enriquecedor se turnan
    /// en vez de pelearse por el bloqueo de SQLite.
    pub write_lock: tokio::sync::Mutex<()>,
    /// Intentos de inicio de sesión por IP y por usuario.
    pub login_limiter: crate::login_limit::LoginLimiter,
}

pub type AppState = Arc<Inner>;

pub fn new(cfg: Config, db: Db, crypto: Crypto) -> AppState {
    let http = crate::http_client::build();
    let subsonic = Subsonic::new(http.clone(), &cfg.navidrome.url);
    let lb = ListenBrainz::new(http.clone(), &cfg.listenbrainz.api_url);
    let lastfm = LastFm::new(http.clone(), &cfg.lastfm.api_url);
    Arc::new(Inner {
        cfg,
        db,
        crypto,
        http,
        subsonic,
        lb,
        lastfm,
        stats_cache: StatsCache::default(),
        lastfm_pending: std::sync::Mutex::new(HashMap::new()),
        now_playing: RwLock::new(HashMap::new()),
        forward_wake: Notify::new(),
        jobs_wake: Notify::new(),
        lastfm_sync_wake: Notify::new(),
        enrich_wake: Notify::new(),
        write_lock: tokio::sync::Mutex::new(()),
        login_limiter: Default::default(),
    })
}
