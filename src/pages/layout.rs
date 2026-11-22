use crate::security::Viewer;
use maud::{DOCTYPE, Markup, html};
pub fn shell(title: &str, path: &str, v: &Viewer, body: Markup) -> String {
    let admin = v.0.as_ref().is_some_and(|u| u.role == "admin");
    static ASSETS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let assets = ASSETS.get_or_init(|| {
        crate::security::hash(concat!(
            include_str!("../../static/app.css"),
            include_str!("../../static/app.js")
        ))[..12]
            .to_string()
    });
    html!{(DOCTYPE)html lang="vi"{head{
        meta charset="utf-8";meta name="viewport" content="width=device-width, initial-scale=1";
        meta name="csrf-token" content=(v.0.as_ref().map(|u|u.csrf.as_str()).unwrap_or(""));
        title{(title)" · EP Lab"}link rel="icon" href="/static/ep-lab-logo.svg";
        link rel="stylesheet" href=(format!("/static/app.css?v={assets}"));script src=(format!("/static/app.js?v={assets}")) defer{}
    }body{
        a class="skip" href="#main"{"Đến nội dung chính"}
        aside class="sidebar" id="sidebar"{
            a class="brand" href="/" aria-label="EP Lab trang chủ"{img src="/static/ep-lab-logo.svg" alt="EP Lab";span{"HỌC · THỰC HÀNH · TIẾN BỘ"}}
            nav aria-label="Điều hướng chính"{
                (nav("/", "Tổng quan",path))
                p class="nav-label"{"KHÔNG GIAN HỌC"}
                @for (track,name,tag) in [("embedded","Lập trình nhúng C","C"),("kicad","Thiết kế mạch KiCad","PCB"),("solidworks","Cơ khí SOLIDWORKS","CAD")]{
                    a class=(if path.contains(track){"nav-item active"}else{"nav-item"}) href=(format!("/lop-hoc/{track}")){span class="nav-mark"{(tag)}(name)}
                }
                p class="nav-label"{"CỘNG ĐỒNG"}
                (nav("/cuoc-thi","Cuộc thi & cơ hội",path))
                (nav("/tham-gia-lab","Tham gia Lab",path))
                @if admin{
                    p class="nav-label"{"NGƯỜI CHỦ TRÌ"}
                    (nav("/quan-tri","Điều hành",path))
                    (nav("/quan-tri/thanh-vien","Duyệt thành viên",path))
                    (nav("/quan-tri/tai-khoan","Quản lý tài khoản",path))
                    (nav("/quan-tri/hoc-lieu","Quản lý học liệu",path))
                    (nav("/quan-tri/nhat-ky","Nhật ký hoạt động",path))
                }
            }
            div class="sidebar-footer"{span class="status-dot"{}"EP Lab · Không gian học kỹ thuật"}
        }
        div class="workspace"{
            header class="topbar"{
                button type="button" class="icon-button mobile-menu" data-menu aria-label="Mở điều hướng" aria-expanded="false"{"☰"}
                div class="breadcrumb"{a href="/"{"EP Lab"}span{" / "}(title)}
                div class="top-actions"{
                    button type="button" class="button compact" data-theme-toggle aria-label="Đổi giao diện sáng tối"{"Sáng / Tối"}
                    @if let Some(u)=&v.0{
                        a class="account-link" href="/tai-khoan"{span class="avatar"{(u.name.chars().next().unwrap_or('E'))}span{(&u.name)}}
                        form data-api="/api/auth/logout" data-method="POST"{button class="icon-button" aria-label="Đăng xuất"{"↪"}}
                    }@else{a class="button compact" href="/dang-nhap"{"Đăng nhập"}}
                }
            }
            main id="main" tabindex="-1"{div id="toast" class="notice" role="status" hidden{}(body)}
            footer class="footer"{span{"EP Lab · Cùng học để làm."}span{"Tham gia miễn phí · Học liệu có nguồn"}}
        }
        dialog id="confirmation" aria-labelledby="confirm-title" aria-describedby="confirm-description"{
            form method="dialog" class="stack"{
                h2 id="confirm-title"{"Xác nhận thao tác"}p id="confirm-description"{}
                div class="actions"{button class="button" value="cancel" autofocus{"Hủy"}button class="button primary" value="confirm" id="confirm-button"{"Xác nhận"}}
            }
        }
    }}}.into_string()
}
fn nav(href: &str, label: &str, current: &str) -> Markup {
    html! {a class=(if href==current{"nav-item active"}else{"nav-item"}) href=(href){(label)}}
}
pub fn heading(eyebrow: &str, title: &str, description: &str) -> Markup {
    html! {div class="page-heading"{div{p class="eyebrow"{(eyebrow)}h1{(title)}p class="muted"{(description)}}}}
}
pub fn input(label: &str, name: &str, kind: &str, value: &str, required: bool) -> Markup {
    html! {label class="field"{span{(label)}input type=(kind) name=(name) value=(value) required[required];}}
}
pub fn area(label: &str, name: &str, value: &str, required: bool, rows: usize) -> Markup {
    html! {label class="field"{span{(label)}textarea name=(name) aria-label=(label) required[required] rows=(rows){(value)}}}
}
pub fn hidden(name: &str, value: impl std::fmt::Display) -> Markup {
    html! {input type="hidden" name=(name) value=(value);}
}
pub fn submit(label: &str) -> Markup {
    html! {button class="button primary" type="submit"{(label)}}
}
pub fn empty(text: &str) -> Markup {
    html! {div class="empty"{p{(text)}}}
}
pub fn track_tabs(track: &str, active: &str) -> Markup {
    html! {nav class="tabs" aria-label="Nội dung môn học"{@for (prefix,label) in [("/lop-hoc","Lớp học"),("/thuc-hanh","Thực hành"),("/thu-vien","Video & giáo trình"),("/trao-doi","Trao đổi")]{a class=(if prefix==active{"selected"}else{""}) href=(format!("{prefix}/{track}")){(label)}}}}
}
pub fn text<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
pub fn strings(v: &serde_json::Value) -> Vec<&str> {
    v.as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default()
}
pub fn date(v: &serde_json::Value) -> String {
    v.as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| {
            d.with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).unwrap())
                .format("%d/%m/%Y %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}
pub fn status(s: &str) -> &str {
    match s {
        "pending" => "Chờ duyệt",
        "approved" => "Đã duyệt",
        "rejected" => "Chưa duyệt",
        "revoked" => "Đã thu hồi",
        "pending_manual" => "Chờ chấm",
        "passed" => "Đạt",
        "needs_revision" => "Cần bổ sung",
        "published" => "Đã xuất bản",
        "draft" => "Bản nháp",
        _ => s,
    }
}
