use std::path::{Component, Path, PathBuf};

use axum::body::Body;
use axum::extract::Path as AxumPath;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::fs;

fn dist_dir() -> PathBuf {
    std::env::var("CONMAN_WEB_DIST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("web/dist"))
}

fn has_parent_navigation(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

async fn file_response(path: PathBuf) -> Response {
    match fs::read(&path).await {
        Ok(bytes) => {
            let mut response = Response::new(Body::from(bytes));
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type_for(&path)),
            );
            response
        }
        Err(_) => (StatusCode::NOT_FOUND, "file not found").into_response(),
    }
}

async fn serve_app_path(path: Option<String>) -> Response {
    let base = dist_dir();

    if let Some(raw_path) = path {
        let cleaned = raw_path.trim_start_matches('/');
        let requested_path = Path::new(cleaned);

        if has_parent_navigation(requested_path) {
            return (
                StatusCode::BAD_REQUEST,
                "invalid app asset path: parent traversal is not allowed",
            )
                .into_response();
        }

        let candidate = base.join(requested_path);
        if candidate.is_file() {
            return file_response(candidate).await;
        }
    }

    let index_path = base.join("index.html");
    if !index_path.is_file() {
        return (
            StatusCode::NOT_FOUND,
            "web UI not built. Run `pnpm --dir web build` first.",
        )
            .into_response();
    }

    file_response(index_path).await
}

pub async fn serve_app_index() -> impl IntoResponse {
    serve_app_path(None).await
}

pub async fn serve_app_asset(AxumPath(path): AxumPath<String>) -> impl IntoResponse {
    serve_app_path(Some(path)).await
}

