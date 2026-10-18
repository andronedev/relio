use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use relio_core::{AnnotationEdit, DeviceInfo, Edited, HIGHLIGHT_COLORS, Library, Stats};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

use crate::covers::{CoverSize, Covers};
use crate::files::{Added, BookFiles, FileError};
use crate::pairing::{PairError, Pairing};
use crate::screen::PairingView;
use crate::update::{self, Outcome, Status, Updater};
use crate::web;

pub struct AppState {
    pub library: Library,
    pub onboard: PathBuf,
    pub version_file: PathBuf,
    pub pairing: Pairing,
    pub files: BookFiles,
    pub covers: Covers,
    pub reading_flag: PathBuf,
    pub port: u16,
    pub updater: Arc<Updater>,
}

const MAX_UPLOAD_BYTES: usize = 1024 * 1024 * 1024;

pub type Shared = Arc<AppState>;

/// Served over the network: the dashboard and the paired-client API.
pub fn public_router(state: Shared) -> Router {
    let api = Router::new()
        .route("/device", get(device))
        .route("/stats", get(stats))
        .route(
            "/books",
            get(books)
                .post(add_book)
                .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
                .delete(delete_book),
        )
        .route("/books/file", get(book_file))
        .route("/books/cover", get(book_cover))
        .route("/annotations", get(annotations))
        .route("/annotations/{id}", axum::routing::patch(edit_annotation).delete(delete_annotation))
        .route("/shelves", get(shelves))
        .route("/clients", get(clients))
        .route("/clients/{id}", delete(revoke_client))
        .layer(middleware::from_fn_with_state(state.clone(), require_client));

    Router::new()
        .route("/api/pair", post(pair))
        .nest("/api", api)
        .merge(web::router())
        .with_state(state)
}

/// Served on the control socket only, which the e-reader UI uses to open pairing codes.
pub fn control_router(state: Shared) -> Router {
    Router::new()
        .route("/pairing", get(current_pairing).post(start_pairing).delete(cancel_pairing))
        .route("/clients", get(clients))
        .route("/clients/{id}", delete(revoke_client))
        .route("/update", get(update_status))
        .route("/update/check", post(check_for_update))
        .route("/update/restart", post(restart_for_update))
        .with_state(state)
}

async fn require_client(State(state): State<Shared>, request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| query_token(request.uri().query()));
    match presented {
        Some(token) if state.pairing.authenticate(token) => next.run(request).await,
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// Plain `<a href>` downloads cannot set headers, so file links carry the token.
fn query_token(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|pair| pair.strip_prefix("token="))
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

const MAX_NOTE_CHARS: usize = 10_000;

enum ApiError {
    NotFound,
    BookOpen,
    InvalidEdit,
    File(FileError),
    Database(BoxError),
    Io(std::io::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
            Self::BookOpen => (StatusCode::CONFLICT, "a book is open on the e-reader").into_response(),
            Self::InvalidEdit => (StatusCode::BAD_REQUEST, "invalid annotation edit").into_response(),
            Self::File(FileError::UnsupportedFormat) => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported format").into_response()
            }
            Self::File(FileError::InvalidName) => (StatusCode::BAD_REQUEST, "invalid file name").into_response(),
            Self::File(FileError::Conversion(err)) => (StatusCode::UNPROCESSABLE_ENTITY, err).into_response(),
            Self::File(FileError::Io(err)) | Self::Io(err) => {
                eprintln!("io error: {err}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::Database(err) => {
                eprintln!("library error: {err}");
                (StatusCode::SERVICE_UNAVAILABLE, "library unavailable").into_response()
            }
        }
    }
}

async fn run_library<T, F>(state: &Shared, query: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&Library) -> Result<T, BoxError> + Send + 'static,
{
    let library = state.library.clone();
    tokio::task::spawn_blocking(move || query(&library))
        .await
        .map_err(|err| ApiError::Database(err.into()))?
        .map_err(ApiError::Database)
}

async fn query_library<T, F>(state: &Shared, query: F) -> Result<Json<T>, ApiError>
where
    T: Serialize + Send + 'static,
    F: FnOnce(&Library) -> Result<T, BoxError> + Send + 'static,
{
    run_library(state, query).await.map(Json)
}

async fn device(State(state): State<Shared>) -> Json<DeviceInfo> {
    let info = tokio::fs::read_to_string(&state.version_file)
        .await
        .map(|contents| DeviceInfo::parse(&contents))
        .unwrap_or_else(|_| DeviceInfo::unknown());
    Json(info)
}

#[derive(Serialize)]
struct UpdateView {
    current: &'static str,
    #[serde(flatten)]
    status: Status,
    last: Option<Outcome>,
}

async fn update_status(State(state): State<Shared>) -> Json<UpdateView> {
    Json(UpdateView { current: update::VERSION, status: state.updater.status(), last: state.updater.last_outcome() })
}

async fn check_for_update(State(state): State<Shared>) -> StatusCode {
    if state.updater.request_check() { StatusCode::ACCEPTED } else { StatusCode::CONFLICT }
}

/// Used by the plugin when Nickel's own restart is not available.
async fn restart_for_update(State(state): State<Shared>) -> StatusCode {
    if !matches!(state.updater.status(), Status::Ready { .. }) {
        return StatusCode::CONFLICT;
    }
    std::thread::spawn(|| {
        let _ = std::process::Command::new("sync").status();
        let _ = std::process::Command::new("reboot").status();
    });
    StatusCode::ACCEPTED
}

async fn stats(State(state): State<Shared>) -> Result<impl IntoResponse, ApiError> {
    let onboard = state.onboard.clone();
    query_library(&state, move |library| Ok(Stats::of(&library.books_on_device(&onboard)?))).await
}

async fn books(State(state): State<Shared>) -> Result<impl IntoResponse, ApiError> {
    let onboard = state.onboard.clone();
    query_library(&state, move |library| Ok(library.books_on_device(&onboard)?)).await
}

async fn shelves(State(state): State<Shared>) -> Result<impl IntoResponse, ApiError> {
    query_library(&state, |library| Ok(library.shelves()?)).await
}

#[derive(Deserialize)]
struct PairRequest {
    code: String,
    name: String,
}

#[derive(Serialize)]
struct PairResponse {
    token: String,
}

async fn pair(State(state): State<Shared>, Json(request): Json<PairRequest>) -> Response {
    match state.pairing.confirm(&request.code, &request.name) {
        Ok(token) => Json(PairResponse { token }).into_response(),
        Err(PairError::WrongCode) => (StatusCode::FORBIDDEN, "wrong code").into_response(),
        Err(PairError::NoPairingInProgress) => (StatusCode::GONE, "no pairing in progress").into_response(),
    }
}

async fn start_pairing(State(state): State<Shared>) -> Json<PairingView> {
    state.pairing.start();
    Json(PairingView::new(state.pairing.state(), state.port))
}

async fn current_pairing(State(state): State<Shared>) -> Json<PairingView> {
    Json(PairingView::new(state.pairing.state(), state.port))
}

async fn cancel_pairing(State(state): State<Shared>) -> StatusCode {
    state.pairing.cancel();
    StatusCode::NO_CONTENT
}

#[derive(Serialize)]
struct ClientView {
    id: String,
    name: String,
    paired_at: u64,
}

async fn clients(State(state): State<Shared>) -> Json<Vec<ClientView>> {
    let views = state
        .pairing
        .clients()
        .into_iter()
        .map(|c| ClientView { id: c.id, name: c.name, paired_at: c.paired_at })
        .collect();
    Json(views)
}

async fn revoke_client(State(state): State<Shared>, Path(id): Path<String>) -> StatusCode {
    if state.pairing.revoke(&id) { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
}

#[derive(Deserialize)]
struct BookFilter {
    book: Option<String>,
}

async fn annotations(
    State(state): State<Shared>,
    Query(filter): Query<BookFilter>,
) -> Result<impl IntoResponse, ApiError> {
    query_library(&state, move |library| Ok(library.annotations(filter.book.as_deref())?)).await
}

#[derive(Deserialize)]
struct BookId {
    id: String,
}

#[derive(Deserialize)]
struct AnnotationChanges {
    note: Option<String>,
    color: Option<u8>,
}

/// Nickel keeps the open book's annotations in memory and would overwrite ours.
fn ensure_no_book_open(state: &AppState) -> Result<(), ApiError> {
    if state.reading_flag.exists() { Err(ApiError::BookOpen) } else { Ok(()) }
}

async fn edit_annotation(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(changes): Json<AnnotationChanges>,
) -> Result<StatusCode, ApiError> {
    let too_long = changes.note.as_ref().is_some_and(|n| n.chars().count() > MAX_NOTE_CHARS);
    let bad_color = changes.color.is_some_and(|c| c >= HIGHLIGHT_COLORS);
    if too_long || bad_color || (changes.note.is_none() && changes.color.is_none()) {
        return Err(ApiError::InvalidEdit);
    }
    ensure_no_book_open(&state)?;
    let edit = AnnotationEdit { note: changes.note.map(|n| n.trim().to_owned()), color: changes.color };
    match run_library(&state, move |library| Ok(library.edit_annotation(&id, &edit)?)).await? {
        Edited::NotFound => Err(ApiError::NotFound),
        _ => Ok(StatusCode::NO_CONTENT),
    }
}

async fn delete_annotation(State(state): State<Shared>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    ensure_no_book_open(&state)?;
    match run_library(&state, move |library| Ok(library.delete_annotation(&id)?)).await? {
        Edited::NotFound => Err(ApiError::NotFound),
        _ => Ok(StatusCode::NO_CONTENT),
    }
}

#[derive(Deserialize)]
struct NewBook {
    name: String,
    #[serde(default)]
    kepub: bool,
}

async fn add_book(
    State(state): State<Shared>,
    Query(NewBook { name, kepub }): Query<NewBook>,
    body: Body,
) -> Result<Json<Added>, ApiError> {
    let (name, staging) = state.files.staging_path(&name).map_err(ApiError::File)?;
    if let Err(err) = receive(&staging, body).await {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(ApiError::Io(err));
    }
    let files = state.files.clone();
    tokio::task::spawn_blocking(move || files.finish(&staging, &name, kepub))
        .await
        .map_err(|err| ApiError::Database(err.into()))?
        .map(Json)
        .map_err(ApiError::File)
}

async fn receive(path: &std::path::Path, mut body: Body) -> std::io::Result<()> {
    let mut file = tokio::fs::File::create(path).await?;
    while let Some(frame) = body.frame().await {
        if let Ok(chunk) = frame.map_err(std::io::Error::other)?.into_data() {
            file.write_all(&chunk).await?;
        }
    }
    file.flush().await
}

/// Removes the file from the e-reader; purchases stay in the Kobo account.
async fn delete_book(State(state): State<Shared>, Query(BookId { id }): Query<BookId>) -> Result<StatusCode, ApiError> {
    let path = state.library.book_file(&id, &state.onboard).ok_or(ApiError::NotFound)?;
    state.files.delete(&path).map_err(ApiError::Io)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CoverQuery {
    id: String,
    #[serde(default)]
    full: bool,
}

async fn book_cover(State(state): State<Shared>, Query(query): Query<CoverQuery>) -> Result<Response, ApiError> {
    let shared = state.clone();
    let path = tokio::task::spawn_blocking(move || {
        let image_id = shared.library.image_id(&query.id).ok().flatten()?;
        let size = if query.full { CoverSize::Full } else { CoverSize::Grid };
        shared.covers.find(&image_id, size)
    })
    .await
    .map_err(|err| ApiError::Database(err.into()))?
    .ok_or(ApiError::NotFound)?;
    let bytes = tokio::fs::read(path).await.map_err(ApiError::Io)?;
    Ok((
        [(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=86400")],
        bytes,
    )
        .into_response())
}

async fn book_file(State(state): State<Shared>, Query(BookId { id }): Query<BookId>) -> Result<Response, ApiError> {
    let path = state.library.book_file(&id, &state.onboard).ok_or(ApiError::NotFound)?;
    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Err(ApiError::NotFound),
        Err(err) => return Err(ApiError::Io(err)),
    };
    let size = file.metadata().await.map_err(ApiError::Io)?.len();
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("book");

    let mut response = Body::from_stream(ReaderStream::new(file)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(size));
    if let Ok(disposition) = HeaderValue::from_str(&format!("attachment; filename*=UTF-8''{}", percent_encode(name))) {
        headers.insert(header::CONTENT_DISPOSITION, disposition);
    }
    Ok(response)
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
