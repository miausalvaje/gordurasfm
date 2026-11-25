//! GordurasFM: estadísticas de escucha autoalojadas para Navidrome.

pub mod api;
pub mod auth;
pub mod config;
pub mod connections;
pub mod crypto;
pub mod db;
pub mod error;
pub mod forward;
pub mod http_client;
pub mod listenbrainz;
pub mod normalize;
pub mod scrobble;
pub mod state;
pub mod subsonic;
pub mod tokens;
pub mod web;

use std::time::Duration;

use axum::{Router, http::HeaderValue};
use tower_http::{
    compression::CompressionLayer, set_header::SetResponseHeaderLayer, timeout::TimeoutLayer, trace::TraceLayer,
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
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(axum::http::StatusCode::REQUEST_TIMEOUT, Duration::from_secs(60)))
        .layer(TraceLayer::new_for_http())
        .with_state(st)
}

/// Arranca las tareas de fondo.
pub fn spawn_background(st: &AppState) {
    tokio::spawn(forward::run(st.clone()));
}
