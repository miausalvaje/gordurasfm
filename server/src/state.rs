//! Estado compartido de la aplicación.

use std::{collections::HashMap, sync::Arc};

use serde::Serialize;
use tokio::sync::{Notify, RwLock};

use crate::{config::Config, crypto::Crypto, db::Db, listenbrainz::client::ListenBrainz, subsonic::Subsonic};

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
    pub now_playing: RwLock<HashMap<i64, NowPlaying>>,
    /// Despierta al reenviador cuando entra algo en la cola.
    pub forward_wake: Notify,
}

pub type AppState = Arc<Inner>;

pub fn new(cfg: Config, db: Db, crypto: Crypto) -> AppState {
    let http = crate::http_client::build();
    let subsonic = Subsonic::new(http.clone(), &cfg.navidrome.url);
    let lb = ListenBrainz::new(http.clone(), &cfg.listenbrainz.api_url);
    Arc::new(Inner {
        cfg,
        db,
        crypto,
        http,
        subsonic,
        lb,
        now_playing: RwLock::new(HashMap::new()),
        forward_wake: Notify::new(),
    })
}
