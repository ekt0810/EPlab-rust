use super::*;
use crate::{AppState, catalog, conversations as api, error::Result, practice, security::Viewer};
use axum::{
    Extension,
    extract::{Path, Query, State},
    response::Html,
};
use maud::{Markup, html};
use serde_json::Value;
use uuid::Uuid;
pub async fn list(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Html<String>> {
    let items = practice::exercises_data(&s, &v, &track).await?;
    let allowed = practice::access(&s, &v, &track).await.is_ok();
    let assignments = if allowed {
        practice::assignments_data(&s, &v, &track, false).await?
    } else {
        vec![]
    };
    let submissions = if allowed {
        practice::submissions_data(&s, &v, &track, false, 1).await?
    } else {
        vec![]
    };
    Ok(page(
        "Thực hành",
        &format!("/thuc-hanh/{track}"),
        &v,
        html! {
        (heading("THỰC HÀNH",catalog::track_name(&track),"Làm sản phẩm, đối chiếu tiêu chí và nộp bằng chứng để nhận phản hồi."))(track_tabs(&track,"/thuc-hanh"))
        @if v.admin().is_ok(){div class="actions"{a class="button primary" href=(format!("/quan-tri/thuc-hanh/{track}")){"Giao & chấm bài"}}}
        @if !assignments.is_empty(){section class="panel article"{h2{"Bài được giao"}@for a in assignments.iter().filter(|a|a["closed"]!=true){div class="list-row"{div{strong{(text(a,"title"))}p class="muted"{(text(a,"instructions"))}"Hạn: "(date(&a["due_at"]))}a class="button compact" href=(format!("/thuc-hanh/{track}/{}?assignment={}",text(a,"exercise_id"),text(a,"id"))){"Làm bài"}}}}}
        div class="resource-grid"{@for e in items{article class="panel article"{span class="badge"{(text(&e,"difficulty"))}h2{(text(&e,"title"))}p class="muted"{(text(&e,"brief"))}div class="actions"{a class="button" href=(format!("/thuc-hanh/{track}/{}",text(&e,"id"))){"Mở bài thực hành"}@if e["completed"]==true{span class="badge success"{"Đã đạt"}}}}}}
        @if allowed{section class="panel article"{h2{"Bài nộp gần đây"}(submission_list(&submissions))}}
        },
    ))
}
#[derive(serde::Deserialize)]
pub struct ExerciseQuery {
    assignment: Option<String>,
}
pub async fn exercise(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
    Query(q): Query<ExerciseQuery>,
) -> Result<Html<String>> {
    let e = match practice::exercise_data(&s, &v, &track, &id).await {
        Ok(e) => e,
        Err(e) => return gate("Bài thực hành", &format!("/thuc-hanh/{track}"), &v, e),
    };
    Ok(page(
        text(&e, "title"),
        &format!("/thuc-hanh/{track}"),
        &v,
        html! {
        a class="back-link" href=(format!("/thuc-hanh/{track}")){"← Thực hành"}(heading("BÀI THỰC HÀNH",text(&e,"title"),text(&e,"brief")))
        div class="two-column"{section class="panel article"{h2{"Yêu cầu"}h3{"Sản phẩm cần nộp"}ul{@for d in strings(&e["deliverables"]){li{(d)}}}h3{"Các bước thực hiện"}ol class="instruction-list"{@for step in strings(&e["steps"]){li{(step)}}}h3{"Tiêu chí đánh giá"}@for criterion in e["criteria"].as_array().unwrap(){div class="list-row"{span{(text(criterion,"text"))}strong{(criterion["points"])"đ"}}}}
        section class="panel article"{h2{"Nộp bài thực hành"}form class="stack" data-api=(format!("/api/practice/{track}/submissions")){
        (hidden("exerciseId",&id))input type="hidden" name="assignmentId" value=(q.assignment.unwrap_or_default()) data-null;
        (area("Mô tả cách làm và kết quả (ít nhất 15 ký tự)","notes","",true,5))
        h3{"Tự kiểm trước khi nộp"}@for (i,c) in e["checks"].as_array().unwrap().iter().enumerate(){fieldset class="self-check"{legend{(c[0].as_str().unwrap_or("Tự kiểm"))}p class="muted"{(c[1].as_str().unwrap_or(""))}input type="hidden" name=(format!("selfChecks.{i}.index")) value=(i) data-type="number";label class="field"{span{"Kết quả"}select name=(format!("selfChecks.{i}.status")){option value="not_run"{"Chưa chạy"}option value="pass"{"Đạt"}option value="fail"{"Chưa đạt"}}}(area("Quan sát thực tế",&format!("selfChecks.{i}.observed"),"",false,2))}}
        label class="field"{span{"Ảnh và tệp nguồn"}input type="file" data-upload multiple;small{"Cần 1–2 ảnh rõ nét (tối thiểu 200×150), tối đa 6 tệp nguồn. Mỗi tệp không quá 10 MB."}}
        div class="uploads"{}(submit("Nộp bài để nhận phản hồi"))
        }}
        }
        },
    ))
}
fn submission_list(data: &[Value]) -> Markup {
    html! {@if data.is_empty(){(empty("Chưa có bài nộp."))}@for item in data{a class="list-row" href=(format!("/bai-nop/{}",text(item,"id"))){div{strong{(text(item,"title"))}p class="muted"{(text(item,"student_name"))" · "(date(&item["created_at"]))}}span class="badge"{(status(text(item,"status")))@if !item["score"].is_null(){" · "(item["score"])"đ"}}}}}
}
pub async fn submission(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>> {
    let d = match practice::detail_data(&s, &v, id).await {
        Ok(d) => d,
        Err(e) => return gate("Bài nộp", "", &v, e),
    };
    let r = &d["rubric"];
    let track = text(&d, "track");
    Ok(page(
        "Bài nộp",
        &format!("/thuc-hanh/{track}"),
        &v,
        html! {
        a class="back-link" href=(format!("/thuc-hanh/{track}")){"← Thực hành"}(heading("BÀI NỘP",text(r,"title"),text(&d,"student_name")))
        div class="two-column"{section class="panel article"{span class="badge"{(status(text(&d,"status")))}h2{"Bằng chứng thực hành"}p class="prose"{(text(&d,"notes"))}
        @for f in d["files"].as_array().unwrap(){div class="evidence"{@if text(f,"content_type").starts_with("image/"){img src=(format!("/api/files/{}",text(f,"id"))) alt=(text(f,"name")) loading="lazy";}a class="file-link" href=(format!("/api/files/{}",text(f,"id"))){(text(f,"name"))" · "(f["size"])" bytes"}}}
        h3{"Tự kiểm của học viên"}@for check in d["self_checks"].as_array().unwrap_or(&vec![]){div class="list-row"{span{(check["index"].as_u64().unwrap_or(0)+1)". "(text(check,"status"))}span{(text(check,"observed"))}}}}
        section class="panel article"{h2{"Nhận xét & kết quả"}@if d["reviews"].as_array().unwrap().is_empty(){(empty("Đang chờ người chủ trì xem và chấm bài."))}
        @for review in d["reviews"].as_array().unwrap(){article class="review"{strong class="score"{(review["result"]["score"])" / 100"}p class="prose"{(text(&review["result"],"summary"))}p class="muted"{(text(review,"reviewer"))" · "(date(&review["created_at"]))}
        @for c in review["result"]["criteria"].as_array().unwrap(){div class="criterion"{h3{(text(c,"text"))}span class="badge"{(match text(c,"status"){"met"=>"Đạt","not_met"=>"Chưa đạt",_=>"Chưa rõ"})}p{(text(c,"evidence"))}@if !text(c,"action").is_empty(){p{"Hướng sửa: "(text(c,"action"))}}}}}}
        }}
        @if v.admin().is_ok(){section class="panel article"{h2{"Chấm theo tiêu chí"}form class="stack" data-api=(format!("/api/admin/practice/submissions/{id}/review")){
        (hidden("revision",d["revision"].as_i64().unwrap_or(1)))(area("Nhận xét chung","summary",text(&d["result"],"summary"),true,4))
        @for (i,c) in r["criteria"].as_array().unwrap().iter().enumerate(){fieldset{legend{(text(c,"text"))" · "(c["points"])" điểm"}(hidden(&format!("criteria.{i}.id"),text(c,"id")))label class="field"{span{"Đánh giá"}select name=(format!("criteria.{i}.status")){option value="unclear"{"Chưa đủ bằng chứng"}option value="met"{"Đạt"}option value="not_met"{"Chưa đạt"}}}(area("Bằng chứng đánh giá",&format!("criteria.{i}.evidence"),"",true,3))(area("Hướng sửa",&format!("criteria.{i}.action"),"",false,2))}}
        (submit("Lưu điểm & phản hồi"))}}}
        },
    ))
}
pub async fn admin(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
    Query(q): Query<practice::Paging>,
) -> Result<Html<String>> {
    if let Err(e) = v.admin() {
        return gate("Quản lý thực hành", "/quan-tri", &v, e);
    }
    let page_n = q.page.unwrap_or(1).max(1);
    let items = practice::exercises_data(&s, &v, &track).await?;
    let assignments = practice::assignments_data(&s, &v, &track, true).await?;
    let submissions = practice::submissions_data(&s, &v, &track, true, page_n).await?;
    let c = s.pool.get().await?;
    let students=crate::db::values(&c,"SELECT jsonb_build_object('id',id,'name',name,'email',email) FROM users WHERE role='student' AND NOT blocked ORDER BY name LIMIT 1000",&[]).await?;
    Ok(page(
        "Quản lý thực hành",
        &format!("/quan-tri/thuc-hanh/{track}"),
        &v,
        html! {(heading("GIAO & CHẤM BÀI",catalog::track_name(&track),"Giao bài cho cả lớp hoặc một học viên. Chấm dựa trên bằng chứng thực tế."))nav class="tabs"{@for t in ["embedded","kicad","solidworks"]{a class=(if t==track{"selected"}else{""}) href=(format!("/quan-tri/thuc-hanh/{t}")){(catalog::track_name(t))}}}
        details class="panel article"{summary{"Giao bài thực hành"}form class="stack narrow" data-api=(format!("/api/admin/practice/{track}/assignments")){label class="field"{span{"Bài thực hành"}select name="exerciseId"{@for e in items{option value=(text(&e,"id")){(text(&e,"title"))}}}}label class="field"{span{"Người nhận"}select name="studentId" data-null{option value=""{"Cả lớp"}@for student in students{option value=(text(&student,"id")){(text(&student,"name"))" · "(text(&student,"email"))}}}}
        (area("Hướng dẫn bổ sung","instructions","",false,3))label class="field"{span{"Hạn nộp (không bắt buộc)"}input type="datetime-local" name="dueAt" data-null;}(submit("Giao bài"))}}
        section class="panel article"{h2{"Các lần giao bài"}@for a in assignments{div class="list-row"{div{strong{(text(&a,"title"))}p class="muted"{(if a["student_id"].is_null(){"Cả lớp"}else{text(&a,"student_name")})" · "(date(&a["due_at"]))}}form data-api=(format!("/api/admin/practice/assignments/{}",text(&a,"id"))) data-method="PATCH"{input type="hidden" name="closed" data-type="boolean" value=(if a["closed"]==true{"false"}else{"true"});button class="button compact"{(if a["closed"]==true{"Mở lại"}else{"Đóng nhận bài"})}}}}}
        section class="panel article"{h2{"Bài nộp"}(submission_list(&submissions))nav class="pagination"{@if page_n>1{a class="button" href=(format!("?page={}",page_n-1)){"Trang trước"}}span{"Trang "(page_n)}@if submissions.len()==30{a class="button" href=(format!("?page={}",page_n+1)){"Trang sau"}}}}
        },
    ))
}
pub async fn conversations(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Html<String>> {
    let data = match api::list_data(&s, &v, &track).await {
        Ok(d) => d,
        Err(e) => return gate("Trao đổi", &format!("/trao-doi/{track}"), &v, e),
    };
    Ok(page(
        "Trao đổi",
        &format!("/trao-doi/{track}"),
        &v,
        html! {(heading("TRAO ĐỔI",catalog::track_name(&track),"Trao đổi riêng giữa học viên và người chủ trì."))(track_tabs(&track,"/trao-doi"))
        details class="panel article"{summary{"Mở chủ đề trao đổi"}form class="stack narrow" data-api=(format!("/api/conversations/{track}")){(input("Chủ đề","subject","text","",true))(area("Nội dung","body","",true,5))(submit("Gửi trao đổi"))}}
        section class="panel article"{@if data.is_empty(){(empty("Chưa có trao đổi. Bạn có thể mở chủ đề mới."))}@for item in data{a class="list-row" href=(format!("/trao-doi/chu-de/{}",text(&item,"id"))){div{h3{(text(&item,"subject"))}p class="muted"{(text(&item,"student_name"))" · "(item["messages"])" tin nhắn"}}span class="badge"{(if item["closed"]==true{"Đã đóng"}else{"Đang mở"})}}}}
        },
    ))
}
pub async fn conversation(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>> {
    let d = match api::detail_data(&s, &v, id).await {
        Ok(d) => d,
        Err(e) => return gate("Trao đổi", "", &v, e),
    };
    Ok(page(
        "Chủ đề trao đổi",
        &format!("/trao-doi/{}", text(&d, "track")),
        &v,
        html! {
        a class="back-link" href=(format!("/trao-doi/{}",text(&d,"track"))){"← Trao đổi"}(heading("TRAO ĐỔI RIÊNG",text(&d,"subject"),text(&d,"student_name")))
        section class="panel article narrow"{div class="messages"{@for m in d["messages"].as_array().unwrap(){article class=(if m["role"]=="admin"{"message host"}else{"message"}){div class="section-title"{strong{(text(m,"author"))}small{(date(&m["created_at"]))}}p class="prose"{(text(m,"body"))}}}}
        @if d["closed"]!=true{form class="stack" data-api=(format!("/api/conversation/{id}/messages")){(hidden("requestId",Uuid::new_v4()))(area("Tin nhắn","body","",true,4))(submit("Gửi trả lời"))}}
        form class="close-thread" data-api=(format!("/api/conversation/{id}")) data-method="PATCH"{(hidden("revision",d["revision"].as_i64().unwrap_or(1)))input type="hidden" name="closed" data-type="boolean" value=(if d["closed"]==true{"false"}else{"true"});button class="button"{(if d["closed"]==true{"Mở lại trao đổi"}else{"Đóng trao đổi"})}}}
        },
    ))
}
