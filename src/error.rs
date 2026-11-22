use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub struct AppError(pub StatusCode, pub String);
pub type Result<T> = std::result::Result<T, AppError>;
impl AppError {
    pub fn bad(s: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, s.into())
    }
    pub fn forbidden(s: impl Into<String>) -> Self {
        Self(StatusCode::FORBIDDEN, s.into())
    }
    pub fn missing(s: impl Into<String>) -> Self {
        Self(StatusCode::NOT_FOUND, s.into())
    }
    pub fn conflict(s: impl Into<String>) -> Self {
        Self(StatusCode::CONFLICT, s.into())
    }
    pub fn login() -> Self {
        Self(StatusCode::UNAUTHORIZED, "Bạn cần đăng nhập.".into())
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
impl From<tokio_postgres::Error> for AppError {
    fn from(e: tokio_postgres::Error) -> Self {
        if let Some(code) = e.code() {
            if *code == tokio_postgres::error::SqlState::UNIQUE_VIOLATION {
                return Self::conflict("Dữ liệu đã tồn tại. Hãy tải lại.");
            }
            if *code == tokio_postgres::error::SqlState::FOREIGN_KEY_VIOLATION {
                return Self::conflict("Dữ liệu còn được tham chiếu hoặc không tồn tại.");
            }
        }
        tracing::error!(code=?e.code(), "database operation failed");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Không thể xử lý dữ liệu lúc này.".into(),
        )
    }
}
impl From<deadpool_postgres::PoolError> for AppError {
    fn from(_: deadpool_postgres::PoolError) -> Self {
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "Kết nối dữ liệu đang bận. Thử lại sau.".into(),
        )
    }
}
impl From<std::io::Error> for AppError {
    fn from(_: std::io::Error) -> Self {
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Không thể đọc hoặc lưu tệp.".into(),
        )
    }
}
