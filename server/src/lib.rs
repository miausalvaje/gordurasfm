//! GordurasFM: estadísticas de escucha autoalojadas para Navidrome.

pub mod admin;
pub mod api;
pub mod auth;
pub mod config;
pub mod connections;
pub mod crypto;
pub mod db;
pub mod enrich;
pub mod error;
pub mod export;
pub mod forward;
pub mod http_client;
pub mod import_file;
pub mod jobs;
pub mod lastfm;
pub mod listenbrainz;
pub mod media;
pub mod normalize;
pub mod periods;
pub mod scrobble;
pub mod settings;
pub mod state;
pub mod stats;
pub mod subsonic;
pub mod tokens;
pub mod web;
pub mod wrapped;

use std::time::Duration;

use axum::{Router, http::HeaderValue};
use tower_http::{
    compression::{
        CompressionLayer,
        predicate::{DefaultPredicate, NotForContentType, Predicate},
    },
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

pub use state::AppState;

/// Router completo: compatibilidad ListenBrainz + API propia + web.
pub fn app(st: AppState) -> Router {
    Router::new()
        .merge(listenbrainz::compat::router())
        .merge(api::router())
        .fallback(web::static_handler)
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::REFERRER_POLICY,
            HeaderValue::from_static("same-origin"),
        ))
        // El audio no se comprime (ya lo está y rompería las peticiones con `Range`).
        .layer(
            CompressionLayer::new().compress_when(DefaultPredicate::new().and(NotForContentType::const_new("audio/"))),
        )
        .layer(TimeoutLayer::with_status_code(axum::http::StatusCode::REQUEST_TIMEOUT, Duration::from_secs(60)))
        .layer(TraceLayer::new_for_http())
        .with_state(st)
}

/// Arranca las tareas de fondo.
pub fn spawn_background(st: &AppState) {
    tokio::spawn(forward::run(st.clone()));
    tokio::spawn(jobs::run(st.clone()));
    tokio::spawn(enrich::run(st.clone()));
    tokio::spawn(db::maintenance(st.db.clone()));
    tokio::spawn(wrapped::run(st.clone()));
}
