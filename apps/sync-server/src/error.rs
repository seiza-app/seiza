use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Why a request was turned away. Each becomes a status and a short JSON `error` the app can
/// show or act on.
#[derive(Debug, PartialEq, Eq)]
pub enum ApiError {
    BadRequest(&'static str),
    /// No token, or one the server doesn't know (e.g. the device was removed).
    Unauthorized,
    Forbidden(&'static str),
    NotFound,
    Conflict(&'static str),
    TooLarge(&'static str),
    TooManyRequests,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unknown device"),
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found"),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::TooLarge(message) => (StatusCode::PAYLOAD_TOO_LARGE, message),
            Self::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many requests, try again in a minute",
            ),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "something went wrong on the server"),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(error: rusqlite::Error) -> Self {
        eprintln!("seiza-sync: database error: {error}");
        Self::Internal
    }
}
