use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

macro_rules! asset {
    ($content_type:expr, $path:literal) => {
        get(|| async {
            (
                [(header::CONTENT_TYPE, $content_type), (header::CACHE_CONTROL, "no-cache")],
                &include_bytes!(concat!("../../../web/", $path))[..],
            )
                .into_response()
        })
    };
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/", asset!("text/html; charset=utf-8", "index.html"))
        .route("/app.js", asset!("text/javascript; charset=utf-8", "app.js"))
        .route("/i18n.js", asset!("text/javascript; charset=utf-8", "i18n.js"))
        .route("/i18n/{file}", get(locale))
        .route("/style.css", asset!("text/css; charset=utf-8", "style.css"))
        .route("/sw.js", asset!("text/javascript; charset=utf-8", "sw.js"))
        .route("/manifest.webmanifest", asset!("application/manifest+json", "manifest.webmanifest"))
        .route("/icon.svg", asset!("image/svg+xml", "icon.svg"))
        .route("/icon-180.png", asset!("image/png", "icons/icon-180.png"))
        .route("/icon-192.png", asset!("image/png", "icons/icon-192.png"))
        .route("/icon-512.png", asset!("image/png", "icons/icon-512.png"))
}

async fn locale(Path(file): Path<String>) -> Response {
    let json = [(header::CONTENT_TYPE, "application/json; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")];
    if file == "index.json" {
        return (json, LOCALE_INDEX).into_response();
    }
    let lang = file.strip_suffix(".json").unwrap_or(&file);
    match LOCALES.iter().find(|(code, _)| *code == lang) {
        Some((_, bytes)) => (json, *bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
