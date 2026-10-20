use axum::{body::Body, http::{header, StatusCode, Uri}, response::{IntoResponse, Response}};
use mime_guess::from_path;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match Assets::get(path) {
        Some(content) => {
            let mime = from_path(path).first_or_octet_stream();
            Response::builder().status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
                .body(Body::from(content.data.into_owned())).unwrap()
        }
        None => match Assets::get("index.html") {
            Some(idx) => {
                let mime = from_path("index.html").first_or_octet_stream();
                Response::builder().status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .body(Body::from(idx.data.into_owned())).unwrap()
            }
            None => (StatusCode::NOT_FOUND, "not found").into_response(),
        },
    }
}
