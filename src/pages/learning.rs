use super::*;
use crate::{AppState, catalog, error::Result, security::Viewer};
use axum::{
    Extension,
    extract::{Path, State},
    response::Html,
};
use maud::html;
pub async fn home(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Html<String>> {
    let courses = catalog::course_index(&s, &v).await?;
    Ok(page(
        "Tổng quan",
        "/",
        &v,
        html! {
        (heading("KHÔNG GIAN HỌC KỸ THUẬT","Bắt đầu từ một điều bạn muốn làm.","Đọc giáo trình, tự thực hành và ghi lại bằng chứng. Tiến bộ được lưu theo từng bài."))
        section class="welcome panel"{div{h2{@if let Some(u)=&v.0{"Chào "(&u.name)"."}@else{"Chào mừng đến EP Lab."}}p{"Ba môn học, một lộ trình rõ ràng. Bạn có thể đọc các bài mở trước khi tham gia Lab."}}a class="button primary" href=(if v.0.as_ref().is_some_and(|u|u.active){"/lop-hoc/embedded"}else{"/tham-gia-lab"}){(if v.0.as_ref().is_some_and(|u|u.active){"Tiếp tục học"}else{"Tham gia miễn phí"})}}
        div class="section-title"{h2{"Chọn môn học"}span class="muted"{"Từ nền tảng đến sản phẩm"}}
        div class="course-grid"{@for (i,c) in courses.iter().enumerate(){article class="panel course-card"{div class=(format!("course-art art-{i}")){span{(if i==0{"{ C }"}else if i==1{"PCB"}else{"CAD"})}}div class="card-body"{p class="eyebrow"{(c["lessonCount"])" BÀI HỌC · "(c["moduleCount"])" HỌC PHẦN"}h2{(text(c,"title"))}p class="muted"{(text(c,"description"))}a class="button" href=(format!("/lop-hoc/{}",text(c,"track"))){"Mở lớp học →"}}}}}
        section class="panel help-strip"{h2{"Một vòng học có kết quả"}ol class="steps-inline"{li{"Đọc & xem ví dụ"}li{"Thực hành từng bước"}li{"Tự kiểm tra"}li{"Nộp bằng chứng"}}}
        },
    ))
}
pub async fn course(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Html<String>> {
    let d = catalog::course_data(&s, &v, &track).await?;
    let lessons = d["lessons"].as_array().unwrap();
    let done = lessons
        .iter()
        .filter(|l| l["progress"]["completed"] == true)
        .count();
    Ok(page(
        text(&d, "title"),
        &format!("/lop-hoc/{track}"),
        &v,
        html! {
        (heading("LỚP HỌC",text(&d,"title"),text(&d,"description")))(track_tabs(&track,"/lop-hoc"))
        div class="progress-summary panel"{div{strong{(done)" / "(lessons.len())}span{" bài đã hoàn thành"}}progress max=(lessons.len()) value=(done) aria-label="Tiến độ môn học"{}}
        @for module in d["modules"].as_array().unwrap(){section class="module"{div class="section-title"{h2{(module["id"])". "(text(module,"title"))}}
        div class="lesson-list panel"{@for l in lessons.iter().filter(|l|l["module"]==module["id"]){a class="lesson-row" href=(format!("/lop-hoc/{track}/{}",text(l,"id"))){span class=(if l["progress"]["completed"]==true{"lesson-state done"}else{"lesson-state"}){(if l["progress"]["completed"]==true{"✓"}else if l["accessible"]==true{"→"}else{"·"})}div{h3{(text(l,"title"))}p class="muted"{(text(l,"summary"))}}span class="lesson-meta"{(l["minutes"])" phút"@if l["preview"]==true{span class="badge"{"Học mở"}}@else if l["accessible"]!=true{span class="badge"{"Thành viên"}}}}}}
        }}
        @if lessons.iter().any(|l|!d["modules"].as_array().unwrap().iter().any(|m|m["id"]==l["module"])){section class="module"{h2{"Bài bổ sung"}div class="lesson-list panel"{@for l in lessons.iter().filter(|l|!d["modules"].as_array().unwrap().iter().any(|m|m["id"]==l["module"])){a class="lesson-row" href=(format!("/lop-hoc/{track}/{}",text(l,"id"))){h3{(text(l,"title"))}}}}}}
        },
    ))
}
pub async fn lesson(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
) -> Result<Html<String>> {
    let d = match catalog::lesson_data(&s, &v, &track, &id).await {
        Ok(d) => d,
        Err(e) => return gate("Bài học", &format!("/lop-hoc/{track}"), &v, e),
    };
    let l = &d["lesson"];
    let p = &d["progress"];
    let path = format!("/lop-hoc/{track}/{id}");
    Ok(page(
        text(l, "title"),
        &path,
        &v,
        html! {
        a class="back-link" href=(format!("/lop-hoc/{track}")){"← "(catalog::track_name(&track))}
        (heading(&format!("HỌC PHẦN {} · {} PHÚT",l["module"],l["minutes"]),text(l,"title"),text(l,"summary")))
        div class="learning-layout"{article class="stack"{
        section class="panel article"{h2{"Sau bài này bạn có thể"}ul{@for objective in strings(&l["objectives"]){li{(objective)}}}}
        section class="panel article"{h2{"Nắm kiến thức"}@for concept in l["concepts"].as_array().unwrap(){h3{(concept[0].as_str().unwrap_or(""))}p class="prose"{(concept[1].as_str().unwrap_or(""))}}}
        @if let Some(code)=l["code"].as_str(){section class="panel article"{h2{"Ví dụ"}pre{code{(code)}}}}
        section class="panel article"{h2{"Tự thực hành"}ol class="instruction-list"{@for step in strings(&l["steps"]){li{(step)}}}}
        @if let Some(example)=l["example"].as_object(){section class="panel article"{h2{(example.get("title").and_then(serde_json::Value::as_str).unwrap_or("Ví dụ"))}p class="prose"{(example.get("body").and_then(serde_json::Value::as_str).unwrap_or(""))}
        @if let Some(rows)=example.get("rows").and_then(serde_json::Value::as_array){div class="table-wrap"{table{tbody{@for row in rows{tr{@for cell in row.as_array().unwrap_or(&vec![]){td{(cell.as_str().map(String::from).unwrap_or_else(||cell.to_string()))}}}}}}}}
        }}
        @if let Some(warnings)=l["mistakes"].as_array(){section class="panel article"{h2{"Lỗi thường gặp"}ul{@for w in warnings{li{(w.as_str().unwrap_or(""))}}}}}
        @if !text(l,"deliverable").is_empty(){section class="panel article"{h2{"Sản phẩm & tự đối chiếu"}p class="prose"{(text(l,"deliverable"))}ul{@for criterion in strings(&l["rubric"]){li{(criterion)}}}@if !text(l,"discussion").is_empty(){h3{"Câu hỏi thảo luận"}p{(text(l,"discussion"))}}}}
        section class="panel article"{h2{"Tự kiểm tra"}p{(text(&l["quiz"],"question"))}
        @if v.auth().is_ok(){form class="stack" data-api=(format!("/api/classroom/{track}/lessons/{id}/quiz")) data-kind="quiz"{@for (i,o) in l["quiz"]["options"].as_array().unwrap().iter().enumerate(){label class="choice"{input type="radio" name="answer" value=(i) data-type="number" required;span{(o.as_str().unwrap_or(""))}}}(submit("Kiểm tra đáp án"))div class="form-result" role="status" hidden{}}}
        @else{a href="/dang-nhap"{"Đăng nhập để tự kiểm và lưu kết quả"}}}
        section class="panel article"{h2{"Tra cứu giáo trình"}p class="muted"{"Tìm đoạn học liệu liên quan có dẫn nguồn. Nội dung được trích từ giáo trình của môn học."}
        @if v.auth().is_ok(){form class="stack" data-api=(format!("/api/classroom/{track}/lessons/{id}/questions")) data-reload="true"{(area("Bạn muốn hiểu thêm điều gì?","question","",true,3))(submit("Tìm trong giáo trình"))}}
        @for q in d["questions"].as_array().unwrap(){div class="answer"{h3{(text(q,"question"))}p class="prose"{(text(&q["answer"],"text"))}@for r in q["answer"]["references"].as_array().unwrap_or(&vec![]){a href=(text(r,"url")){(text(r,"title"))}}}}
        }}aside class="stack lesson-aside"{
        section class="panel article"{h2{"Tiến độ của bạn"}@if v.auth().is_ok(){form class="stack" data-api=(format!("/api/classroom/{track}/lessons/{id}/progress")) data-method="PUT"{
        @for (i,step) in strings(&l["steps"]).iter().enumerate(){label class="check"{input type="checkbox" name="checklist[]" value=(i) data-type="number" checked[p["checklist"].as_array().is_some_and(|a|a.contains(&serde_json::json!(i)))];span{(step)}}}
        (hidden("scene",p["scene"].as_i64().unwrap_or(0)))(area("Ghi chú riêng","note",text(p,"note"),false,5))
        p class="quiz-state"{(if p["quiz_passed"]==true{"✓ Đã vượt qua tự kiểm"}else{"○ Chưa vượt qua tự kiểm"})}
        label class="check"{input type="checkbox" name="completed" checked[p["completed"]==true];span{"Đánh dấu hoàn thành bài"}}
        (submit("Lưu tiến độ"))}}
        @else{p{"Đăng nhập để lưu tiến độ, ghi chú và câu hỏi."}a class="button primary" href="/dang-nhap"{"Đăng nhập"}}}
        @if l["visualGuide"].is_object(){section class="panel article"{h2{"Khi xem video"}ul{@for item in strings(&l["visualGuide"]["watchFor"]){li{(item)}}}p class="prose"{(text(&l["visualGuide"],"tryIt"))}}}
        @if !d["sources"].as_array().unwrap().is_empty(){section class="panel article"{h2{"Video tham khảo"}@for video in d["sources"].as_array().unwrap(){div class="video-item"{h3{(text(video,"title"))}p class="muted"{(text(video,"channel"))}a class="button compact" href=(text(video,"url")) target="_blank" rel="noopener noreferrer"{"Xem trên YouTube ↗"}}}}}
        @if !strings(&l["sources"]).is_empty(){section class="panel article"{h2{"Nguồn tài liệu"}@for source in strings(&l["sources"]){a class="file-link" href=(source) target="_blank" rel="noopener noreferrer"{(source)}}}}
        @if let Some(resources)=d["resources"].as_array(){@if !resources.is_empty(){section class="panel article"{h2{"Tệp bài học"}@for f in resources{a class="file-link" href=(format!("/api/files/{}",text(f,"id"))){(text(f,"name"))}}}}}
        }}
        },
    ))
}
pub async fn library(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Html<String>> {
    catalog::valid_track(&track)?;
    let sources = s.content.sources["sources"].as_array().unwrap();
    Ok(page(
        "Video & giáo trình",
        &format!("/thu-vien/{track}"),
        &v,
        html! {
        (heading("THƯ VIỆN",catalog::track_name(&track),"Học liệu và video có ghi nguồn tác giả."))(track_tabs(&track,"/thu-vien"))
        section class="panel article"{h2{"Giáo trình EP Lab"}p{"Đọc theo học phần, làm bài tập rồi lưu bằng chứng."}div class="actions"{a class="button primary" href=(format!("/api/classroom/{track}/handbook")) target="_blank"{"Mở giáo trình PDF"}@if v.admin().is_ok(){a class="button" href=(format!("/api/admin/handbooks/{track}")) target="_blank"{"Tài liệu người chủ trì"}}}}
        div class="resource-grid"{@for video in sources.iter().filter(|x|x["track"]==track&&x["kind"]!="excluded"&&x["kind"]!="unavailable"){article class="panel article"{span class="badge"{(text(video,"channel"))}h3{(text(video,"title"))}p class="muted"{(video["duration"].as_u64().unwrap_or(0)/60)" phút"}a href=(text(video,"url")) target="_blank" rel="noopener noreferrer"{"Xem nguồn ↗"}}}}
        },
    ))
}
