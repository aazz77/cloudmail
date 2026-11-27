use axum::extract::State;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::models::AppState;

pub async fn init(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    // 密钥从 header 传入，避免出现在 URL / 日志 / 浏览器历史中。
    let secret = headers
        .get("x-init-secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if secret != state.config.jwt_secret {
        return (axum::http::StatusCode::UNAUTHORIZED, "JWT secret mismatch").into_response();
    }
    (axum::http::StatusCode::OK, "success").into_response()
}
