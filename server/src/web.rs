//! Web estática (Flutter) embebida en el binario, con vuelta a `index.html` para las rutas de la SPA.

use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

/// Se dejan fuera los símbolos de depuración y los motores que solo usa la compilación `--wasm`.
#[derive(RustEmbed)]
#[folder = "../web/build/web"]
#[exclude = "*.symbols"]
#[exclude = "*.map"]
#[exclude = "canvaskit/skwasm*"]
#[exclude = "canvaskit/wimp*"]
#[exclude = "canvaskit/webparagraph/*"]
struct Assets;

pub async fn static_handler(uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path.starts_with("1/") {
        return (StatusCode::NOT_FOUND, "no encontrado").into_response();
    }
    if let Some(r) = serve(path, &headers) {
        return r;
    }
    // Rutas de la SPA (go_router): se devuelve index.html.
    serve("index.html", &headers).unwrap_or_else(|| (StatusCode::NOT_FOUND, "no encontrado").into_response())
}

fn serve(path: &str, headers: &HeaderMap) -> Option<Response> {
    let path = if path.is_empty() { "index.html" } else { path };
    let file = Assets::get(path)?;
    let etag = format!("\"{}\"", hex::encode(&file.metadata.sha256_hash()[..12]));
    // Los nombres de Flutter no llevan hash: el navegador revalida siempre con la ETag (respuesta 304, sin cuerpo).
    let cached = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag));
    let builder = Response::builder().header(header::ETAG, &etag).header(header::CACHE_CONTROL, "no-cache");
    if cached {
        return Some(builder.status(StatusCode::NOT_MODIFIED).body(Body::empty()).unwrap());
    }
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Some(builder.header(header::CONTENT_TYPE, mime.as_ref()).body(Body::from(file.data.into_owned())).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etag_y_304() {
        let r = serve("index.html", &HeaderMap::new()).unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let etag = r.headers()[header::ETAG].clone();
        let mut h = HeaderMap::new();
        h.insert(header::IF_NONE_MATCH, etag);
        assert_eq!(serve("index.html", &h).unwrap().status(), StatusCode::NOT_MODIFIED);
        assert!(serve("canvaskit/skwasm.wasm", &HeaderMap::new()).is_none());
    }
}
