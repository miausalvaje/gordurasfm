//! Web estática (Flutter) embebida en el binario, con vuelta a `index.html` para las rutas de la SPA.

use axum::{
    body::Body,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../web/build/web"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path.starts_with("1/") {
        return (StatusCode::NOT_FOUND, "no encontrado").into_response();
    }
    if let Some(r) = serve(path) {
        return r;
    }
    // Rutas de la SPA (go_router): se devuelve index.html.
    serve("index.html").unwrap_or_else(|| (StatusCode::NOT_FOUND, "no encontrado").into_response())
}

fn serve(path: &str) -> Option<Response> {
    let path = if path.is_empty() { "index.html" } else { path };
    let file = Assets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // index.html y el service worker no se cachean; el resto, una hora (Flutter no pone hash en todos los nombres).
    let cache = if path == "index.html" || path.ends_with("flutter_service_worker.js") || path.ends_with("version.json")
    {
        "no-cache"
    } else {
        "public, max-age=3600"
    };
    Some(
        Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .header(header::CACHE_CONTROL, cache)
            .body(Body::from(file.data.into_owned()))
            .unwrap(),
    )
}
