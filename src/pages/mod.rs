mod account;
mod layout;
mod learning;
mod management;
mod training;
use crate::{
    AppState,
    error::{AppError, Result},
    security::Viewer,
};
use axum::{
    Extension, Router,
    extract::Path,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
pub use layout::*;
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(learning::home))
        .route("/lop-hoc/{track}", get(learning::course))
        .route("/lop-hoc/{track}/{id}", get(learning::lesson))
        .route("/thu-vien/{track}", get(learning::library))
        .route("/dang-nhap", get(account::login))
        .route("/dang-ky", get(account::register))
        .route("/xac-minh", get(account::verify))
        .route("/quen-mat-khau", get(account::forgot))
        .route("/khoi-phuc", get(account::reset))
        .route("/nhan-tai-khoan", get(account::invite))
        .route("/tai-khoan", get(account::profile))
        .route("/tham-gia-lab", get(account::membership))
        .route("/quan-tri", get(management::overview))
        .route("/quan-tri/thanh-vien", get(management::memberships))
        .route("/quan-tri/tai-khoan", get(management::accounts))
        .route("/quan-tri/tai-khoan/{id}", get(management::account))
        .route("/quan-tri/hoc-lieu", get(management::content))
        .route("/quan-tri/hoc-lieu/{track}/{id}", get(management::editor))
        .route("/quan-tri/nhat-ky", get(management::audit))
        .route("/quan-tri/cuoc-thi", get(management::competitions))
        .route("/cuoc-thi", get(management::public_competitions))
        .route("/thuc-hanh/{track}", get(training::list))
        .route("/thuc-hanh/{track}/{id}", get(training::exercise))
        .route("/bai-nop/{id}", get(training::submission))
        .route("/quan-tri/thuc-hanh/{track}", get(training::admin))
        .route("/trao-doi/{track}", get(training::conversations))
        .route("/trao-doi/chu-de/{id}", get(training::conversation))
        .route("/static/{name}", get(static_file))
        .fallback(not_found)
}
async fn static_file(Path(name): Path<String>) -> Response {
    let (mime, data) = match name.as_str() {
        "app.css" => (
            "text/css; charset=utf-8",
            include_str!("../../static/app.css"),
        ),
        "app.js" => (
            "text/javascript; charset=utf-8",
            include_str!("../../static/app.js"),
        ),
        "ep-lab-logo.svg" => (
            "image/svg+xml",
            include_str!("../../static/ep-lab-logo.svg"),
        ),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        data,
    )
        .into_response()
}
async fn not_found(Extension(v): Extension<Viewer>) -> Response {
    (StatusCode::NOT_FOUND,Html(shell("Không tìm thấy trang","",&v,maud::html!{section class="empty panel"{h1{"Trang này không tồn tại"}p{"Hãy chọn môn học từ thanh điều hướng."}a class="button primary" href="/"{"Về trang chủ"}}}))).into_response()
}
pub fn page(title: &str, path: &str, v: &Viewer, body: maud::Markup) -> Html<String> {
    Html(shell(title, path, v, body))
}
pub fn gate(title: &str, path: &str, v: &Viewer, error: AppError) -> Result<Html<String>> {
    if matches!(error.0, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        Ok(page(
            title,
            path,
            v,
            maud::html! {section class="panel empty"{span class="eyebrow"{"QUYỀN TRUY CẬP"}h1{(title)}p{(error.1)}div class="actions"{@if v.0.is_none(){a class="button primary" href="/dang-nhap"{"Đăng nhập"}}@else{a class="button primary" href="/tham-gia-lab"{"Tham gia Lab"}}a class="button" href="/"{"Về trang chủ"}}}},
        ))
    } else {
        Err(error)
    }
}
