use super::*;
use crate::{
    AppState, admin as api, db,
    error::{AppError, Result},
    membership,
    security::Viewer,
};
use axum::{
    Extension,
    extract::{Path, Query, State},
    response::Html,
};
use maud::{Markup, html};
use serde_json::{Value, json};
use uuid::Uuid;
pub async fn overview(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Html<String>> {
    let d = match api::overview_data(&s, &v).await {
        Ok(d) => d,
        Err(e) => return gate("Điều hành", "/quan-tri", &v, e),
    };
    Ok(page(
        "Điều hành",
        "/quan-tri",
        &v,
        html! {(heading("NGƯỜI CHỦ TRÌ","Hôm nay cần xử lý gì?","Theo dõi việc học, duyệt thành viên và phản hồi bài thực hành."))div class="stats"{@for (key,label,link) in [("pending","Yêu cầu đang chờ","/quan-tri/thanh-vien"),("submissions","Bài chờ chấm","/quan-tri/thuc-hanh/embedded"),("members","Thành viên","/quan-tri/thanh-vien?status=approved"),("students","Tài khoản học viên","/quan-tri/tai-khoan")]{a class="panel stat" href=(link){span{(label)}strong{(d[key])}}}}
        div class="two-column"{section class="panel article"{h2{"Lớp học & thực hành"}@for (track,label) in [("embedded","Lập trình nhúng C"),("kicad","Thiết kế mạch KiCad"),("solidworks","Cơ khí SOLIDWORKS")]{div class="list-row"{strong{(label)}div class="actions"{a class="button compact" href=(format!("/quan-tri/thuc-hanh/{track}")){"Giao / chấm bài"}a href=(format!("/trao-doi/{track}")){"Trao đổi"}}}}}
        section class="panel article"{h2{"Quản lý chung"}a class="list-row" href="/quan-tri/hoc-lieu"{"Học liệu & giáo trình →"}a class="list-row" href="/quan-tri/cuoc-thi"{"Cuộc thi & cơ hội →"}a class="list-row" href="/quan-tri/nhat-ky"{"Nhật ký hoạt động →"}p class="muted"{(d["lessons"])" bài đang xuất bản · "(d["conversations"])" trao đổi đang mở"}}}
        },
    ))
}
pub async fn memberships(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Query(q): Query<membership::Filter>,
) -> Result<Html<String>> {
    let selected = q.status.clone().unwrap_or_else(|| "pending".into());
    let query = q.query.clone().unwrap_or_default();
    let d = match membership::list_data(&s, &v, q).await {
        Ok(d) => d,
        Err(e) => return gate("Duyệt thành viên", "/quan-tri/thanh-vien", &v, e),
    };
    Ok(page(
        "Duyệt thành viên",
        "/quan-tri/thanh-vien",
        &v,
        html! {
        (heading("THÀNH VIÊN","Duyệt yêu cầu tham gia","Duyệt một lần để mở quyền học đầy đủ. Phản hồi rõ lý do khi chưa duyệt hoặc thu hồi."))
        form class="filters" method="get"{label class="field"{span{"Trạng thái"}select name="status" aria-label="Trạng thái"{@for key in ["pending","approved","rejected","revoked","all"]{option value=(key) selected[key==selected]{(if key=="all"{"Tất cả"}else{status(key)})}}}}(input("Tìm tên, email, điện thoại","query","search",&query,false))(submit("Lọc"))}
        p class="muted"{(d["total"])" yêu cầu"}
        div class="stack"{@for r in d["items"].as_array().unwrap(){article class="panel article"{div class="section-title"{div{h2{a href=(format!("/quan-tri/tai-khoan/{}",text(r,"user_id"))){(text(r,"name"))}}p class="muted"{(text(r,"email"))" · "(text(r,"phone"))}}span class="badge"{(status(text(r,"status")))}}p class="prose"{(text(r,"message"))}p class="muted"{(strings(&r["tracks"]).iter().map(|t|crate::catalog::track_name(t)).collect::<Vec<_>>().join(" · "))}
        @if r["status"]=="pending"{div class="actions"{form data-api=(format!("/api/admin/lab-memberships/{}",text(r,"user_id"))) data-method="PATCH"{(hidden("status","approved"))(hidden("revision",r["revision"].as_i64().unwrap_or(1)))(submit("Duyệt tham gia"))}
        details class="decision"{summary{"Chưa duyệt"}form class="stack" data-api=(format!("/api/admin/lab-memberships/{}",text(r,"user_id"))) data-method="PATCH"{(hidden("status","rejected"))(hidden("revision",r["revision"].as_i64().unwrap_or(1)))(area("Lý do chưa duyệt","note","",true,3))(submit("Lưu phản hồi"))}}}}
        @else if r["status"]=="approved"{details class="decision"{summary{"Thu hồi quyền Lab"}form class="stack" data-api=(format!("/api/admin/lab-memberships/{}",text(r,"user_id"))) data-method="PATCH" data-confirm="Thu hồi quyền học của thành viên này?"{(hidden("status","revoked"))(hidden("revision",r["revision"].as_i64().unwrap_or(1)))(area("Lý do thu hồi","note","",true,3))(submit("Thu hồi quyền"))}}}
        @if !text(r,"decision_note").is_empty(){p class="muted"{(text(r,"decision_note"))}}
        }}
        @if d["items"].as_array().unwrap().is_empty(){(empty("Không có yêu cầu trong bộ lọc này."))}}
        (pagination("/quan-tri/thanh-vien",&d,&format!("status={}&query={}",enc(&selected),enc(&query)),30))
        },
    ))
}
pub fn enc(v: &str) -> String {
    url::form_urlencoded::byte_serialize(v.as_bytes()).collect()
}
fn pagination(path: &str, d: &Value, query: &str, size: i64) -> Markup {
    let p = d["page"].as_i64().unwrap_or(1);
    html! {nav class="pagination" aria-label="Phân trang"{@if p>1{a class="button" href=(format!("{path}?{query}&page={}",p-1)){"← Trang trước"}}span{"Trang "(p)}@if p*size<d["total"].as_i64().unwrap_or(0){a class="button" href=(format!("{path}?{query}&page={}",p+1)){"Trang sau →"}}}}
}
pub async fn accounts(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Query(q): Query<api::Search>,
) -> Result<Html<String>> {
    let search = q.q.clone().unwrap_or_default();
    let selected = q.state.clone().unwrap_or_else(|| "all".into());
    let d = match api::accounts_data(&s, &v, q).await {
        Ok(d) => d,
        Err(e) => return gate("Quản lý tài khoản", "/quan-tri/tai-khoan", &v, e),
    };
    Ok(page(
        "Quản lý tài khoản",
        "/quan-tri/tai-khoan",
        &v,
        html! {(heading("TÀI KHOẢN","Hồ sơ học viên","Tìm kiếm, gửi thư hỗ trợ và quản lý trạng thái tài khoản."))
        details class="panel article"{summary{"Tạo hồ sơ & gửi thư mời"}form class="stack narrow" data-api="/api/admin/accounts"{(input("Họ tên","name","text","",true))(input("Email","email","email","",true))(input("Số điện thoại","phone","tel","",true))(area("Ghi chú (không bắt buộc)","note","",false,2))(submit("Tạo và gửi thư mời"))}}
        form class="filters" method="get"{(input("Tìm tài khoản","q","search",&search,false))label class="field"{span{"Trạng thái"}select name="state"{@for (key,label) in [("all","Tất cả"),("active","Đang có quyền học"),("blocked","Đã khóa"),("unverified","Chưa xác minh"),("invited","Chờ nhận lời mời")]{option value=(key) selected[key==selected]{(label)}}}}(submit("Tìm kiếm"))}
        div class="panel table-wrap"{table{thead{tr{th{"Họ tên"}th{"Liên hệ"}th{"Trạng thái"}th{"Thao tác"}}}tbody{@for u in d["items"].as_array().unwrap(){tr{td{strong{(text(u,"name"))}@if u["role"]=="admin"{span class="badge"{"Người chủ trì"}}}td{(text(u,"email"))br;(text(u,"phone"))}td{(if u["blocked"]==true{"Đã khóa"}else if u["setup_pending"]==true{"Chờ nhận lời mời"}else if u["verified"]!=true{"Chưa xác minh"}else if u["active"]==true{"Thành viên"}else{"Đã xác minh"})}td{a class="button compact" href=(format!("/quan-tri/tai-khoan/{}",text(u,"id"))){"Mở hồ sơ"}}}}}}}
        (pagination("/quan-tri/tai-khoan",&d,&format!("q={}&state={}",enc(&search),enc(&selected)),25))
        },
    ))
}
pub async fn account(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>> {
    let d = match api::account_data(&s, &v, id).await {
        Ok(d) => d,
        Err(e) => return gate("Hồ sơ học viên", "/quan-tri/tai-khoan", &v, e),
    };
    let u = &d["user"];
    Ok(page(
        "Hồ sơ học viên",
        "/quan-tri/tai-khoan",
        &v,
        html! {a class="back-link" href="/quan-tri/tai-khoan"{"← Danh sách tài khoản"}(heading("HỒ SƠ HỌC VIÊN",text(u,"name"),text(u,"email")))
        div class="two-column"{section class="panel article stack"{h2{"Thông tin & hỗ trợ"}dl class="facts"{dt{"Điện thoại"}dd{(text(u,"phone"))}dt{"Quyền học"}dd{(if u["active"]==true{"Đã mở"}else{"Chưa mở"})}dt{"Xác minh email"}dd{(if u["verified"]==true{"Hoàn tất"}else{"Chưa xác minh"})}dt{"Phiên hiện có"}dd{(d["sessions"].as_array().unwrap().len())}}
        @if u["role"]=="student"{form data-api=(format!("/api/admin/accounts/{id}/email")){(hidden("purpose",if u["setup_pending"]==true{"invite"}else if u["verified"]==true{"reset"}else{"verify"}))(submit(if u["setup_pending"]==true{"Gửi lại thư mời"}else if u["verified"]==true{"Gửi email khôi phục"}else{"Gửi email xác minh"}))}
        form class="stack" data-api=(format!("/api/admin/accounts/{id}/notes")){(area("Ghi chú hỗ trợ","note","",true,3))(submit("Lưu ghi chú"))}
        details{summary{(if u["blocked"]==true{"Mở khóa tài khoản"}else{"Khóa tài khoản"})}form class="stack" data-api=(format!("/api/admin/users/{id}")) data-method="PATCH" data-confirm=(if u["blocked"]==true{"Mở lại tài khoản này?"}else{"Khóa tài khoản và đăng xuất tất cả phiên?"}){input type="hidden" name="blocked" value=(if u["blocked"]==true{"false"}else{"true"}) data-type="boolean";(area("Lý do","reason","",u["blocked"]!=true,3))(submit(if u["blocked"]==true{"Mở khóa"}else{"Khóa tài khoản"}))}}
        details{summary{"Đăng xuất các phiên"}form class="stack" data-api=(format!("/api/admin/accounts/{id}/revoke-sessions")) data-confirm="Đăng xuất các phiên hiện tại của học viên?"{(area("Lý do","reason","",true,3))(submit("Đăng xuất các phiên"))}}}}
        section class="panel article"{h2{"Tiến độ học tập"}@if d["progress"].as_array().unwrap().is_empty(){(empty("Chưa có tiến độ."))}@for p in d["progress"].as_array().unwrap(){div class="list-row"{strong{(crate::catalog::track_name(text(p,"track")))}span{(p["completed"])" / "(p["started"])" bài hoàn tất"}}}h3{"Yêu cầu Lab"}p{(status(text(&d["membership"],"status")))}p class="prose"{(text(&d["membership"],"message"))}a href="/quan-tri/thanh-vien?status=all"{"Mở danh sách yêu cầu"}}}
        section class="panel article"{h2{"Lịch sử hồ sơ"}(history(&d["history"]))}
        },
    ))
}
fn history(data: &Value) -> Markup {
    html! {div class="timeline"{@for entry in data.as_array().unwrap_or(&vec![]){article{div class="section-title"{strong{(text(entry,"action"))}small{(date(&entry["created_at"]))}}p class="muted"{(text(entry,"actor_name"))}p class="prose wrap"{(entry["metadata"].to_string())}}}}}
}
pub async fn audit(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Query(q): Query<api::Search>,
) -> Result<Html<String>> {
    let n = q.page.unwrap_or(1).max(1);
    let d = match api::audit_data(&s, &v, n).await {
        Ok(d) => d,
        Err(e) => return gate("Nhật ký hoạt động", "/quan-tri/nhat-ky", &v, e),
    };
    Ok(page(
        "Nhật ký hoạt động",
        "/quan-tri/nhat-ky",
        &v,
        html! {(heading("NHẬT KÝ","Ai đã thay đổi điều gì?","Theo dõi tài khoản, quyền học và các quyết định quản lý."))section class="panel article"{(history(&json!(d)))}nav class="pagination"{@if n>1{a class="button" href=(format!("?page={}",n-1)){"Trang trước"}}span{"Trang "(n)}@if d.len()==50{a class="button" href=(format!("?page={}",n+1)){"Trang sau"}}}},
    ))
}
pub async fn content(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Html<String>> {
    let courses = match api::courses_data(&s, &v).await {
        Ok(d) => d,
        Err(e) => return gate("Quản lý học liệu", "/quan-tri/hoc-lieu", &v, e),
    };
    let lessons = api::lessons_data(&s, &v).await?;
    Ok(page(
        "Quản lý học liệu",
        "/quan-tri/hoc-lieu",
        &v,
        html! {(heading("HỌC LIỆU","Môn học & bài học","Lưu bản nháp hoặc xuất bản. Các thay đổi được ghi vào nhật ký."))
        @for c in courses{section class="panel article module"{div class="section-title"{h2{(text(&c,"title"))}span class="badge"{(status(text(&c,"status")))}}details{summary{"Sửa thông tin môn học"}form class="stack" data-api=(format!("/api/admin/courses/{}",text(&c,"category"))) data-method="PUT"{(hidden("revision",c["revision"].as_i64().unwrap_or(1)))(input("Tên môn","title","text",text(&c,"title"),true))(area("Mô tả","description",text(&c,"description"),true,3))(publication(text(&c,"status")))(submit("Lưu môn học"))}}
        div class="actions"{a class="button" href=(format!("/quan-tri/hoc-lieu/{}/new",text(&c,"category"))){"Thêm bài học"}a href=(format!("/lop-hoc/{}",text(&c,"category"))){"Xem lớp học →"}}
        @for l in lessons.iter().filter(|l|l["track"]==c["category"]){div class="list-row"{div{strong{(text(&l["body"],"title"))}small class="muted"{" · "(text(l,"id"))" · "(status(text(l,"status")))}}a class="button compact" href=(format!("/quan-tri/hoc-lieu/{}/{}",text(l,"track"),text(l,"id"))){"Chỉnh sửa"}}}
        }}
        },
    ))
}
fn publication(selected: &str) -> Markup {
    html! {label class="field"{span{"Trạng thái"}select name="status" aria-label="Trạng thái"{option value="draft" selected[selected=="draft"]{"Bản nháp"}option value="published" selected[selected=="published"]{"Xuất bản"}}}}
}
pub async fn editor(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
) -> Result<Html<String>> {
    if let Err(e) = v.admin() {
        return gate("Chỉnh sửa bài học", "/quan-tri/hoc-lieu", &v, e);
    }
    crate::catalog::valid_track(&track)?;
    let new = id == "new";
    let c = s.pool.get().await?;
    let row = if new {
        json!({"revision":0,"status":"draft","body":{"id":"bai-moi","title":"Bài học mới","summary":"Mô tả nội dung bài học","module":1,"minutes":30,"level":"Cơ bản","preview":false,"concepts":[["Khái niệm","Giải thích khái niệm"]],"objectives":["Mục tiêu bài học"],"steps":["Thực hành bước đầu"],"quiz":{"question":"Câu hỏi tự kiểm?","options":["Lựa chọn A","Lựa chọn B"],"answer":0,"explanation":"Giải thích đáp án"},"videos":[]}})
    } else {
        let row = db::value(
            &c,
            "SELECT to_jsonb(l) FROM lessons l WHERE track=$1 AND id=$2",
            &[&track, &id],
        )
        .await?;
        if row.is_null() {
            return Err(AppError::missing("Không tìm thấy bài học."));
        }
        row
    };
    let body = &row["body"];
    let resources = if new {
        vec![]
    } else {
        db::values(&c,"SELECT jsonb_build_object('id',id,'name',name) FROM files WHERE lesson_track=$1 AND lesson_id=$2",&[&track,&id]).await?
    };
    Ok(page(
        "Chỉnh sửa bài học",
        "/quan-tri/hoc-lieu",
        &v,
        html! {a class="back-link" href="/quan-tri/hoc-lieu"{"← Học liệu"}(heading("BIÊN TẬP",if new{"Thêm bài học"}else{text(body,"title")},"Chỉnh sửa nội dung rồi lưu. Bản nháp chỉ người chủ trì xem được."))
        section class="panel article"{form class="stack" data-kind="lesson" data-api=(if new{"/api/admin/lessons".into()}else{format!("/api/admin/lessons/{track}/{id}")}) data-method=(if new{"POST"}else{"PUT"}){
        (hidden("track",&track))(hidden("revision",row["revision"].as_i64().unwrap_or(0)))
        label class="field"{span{"Mã bài"}input name="id" value=(text(body,"id")) required readonly[!new];}
        (input("Tên bài học","title","text",text(body,"title"),true))(area("Tóm tắt","summary",text(body,"summary"),true,3))
        div class="two-column"{(input("Học phần","module","number",&body["module"].to_string(),true))(input("Thời lượng (phút)","minutes","number",&body["minutes"].to_string(),true))}
        label class="check"{input type="checkbox" name="preview" checked[body["preview"]==true];span{"Bài học mở (không cần quyền thành viên)"}}
        (publication(text(&row,"status")))
        details{summary{"Nội dung chi tiết"}div class="stack"{
        (area("Mục tiêu (mỗi dòng một mục)","lessonObjectives",&strings(&body["objectives"]).join("\n"),true,4))
        (area("Khái niệm","lessonConcepts",&body["concepts"].as_array().unwrap().iter().map(|c|format!("{}\n{}",c[0].as_str().unwrap_or(""),c[1].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n\n"),true,12))
        p class="muted"{"Mỗi khái niệm là một khối: dòng đầu là tên, các dòng sau là giải thích. Ngăn các khối bằng một dòng trống."}
        (area("Các bước thực hành (mỗi dòng một bước)","lessonSteps",&strings(&body["steps"]).join("\n"),true,6))
        (area("Lỗi thường gặp (mỗi dòng một lỗi)","lessonMistakes",&strings(&body["mistakes"]).join("\n"),false,4))
        (area("Sản phẩm cần hoàn thành","lessonDeliverable",text(body,"deliverable"),false,3))
        (area("Nguồn HTTPS (mỗi dòng một liên kết)","lessonSources",&strings(&body["sources"]).join("\n"),false,3))
        }}
        details{summary{"Câu hỏi tự kiểm"}div class="stack"{
        (area("Câu hỏi","quizQuestion",text(&body["quiz"],"question"),true,3))
        (area("Các lựa chọn (mỗi dòng một lựa chọn)","quizOptions",&strings(&body["quiz"]["options"]).join("\n"),true,4))
        (input("Số thứ tự đáp án đúng (bắt đầu từ 1)","quizAnswer","number",&(body["quiz"]["answer"].as_u64().unwrap_or(0)+1).to_string(),true))
        (area("Giải thích đáp án","quizExplanation",text(&body["quiz"],"explanation"),false,4))
        }}
        textarea name="body" data-json hidden aria-label="Dữ liệu gốc"{(body.to_string())}
        (submit("Lưu bài học"))}}
        @if !new{section class="panel article"{h2{"Tệp học liệu"}@for f in resources{div class="list-row"{a href=(format!("/api/files/{}",text(&f,"id"))){(text(&f,"name"))}form data-api=(format!("/api/files/{}",text(&f,"id"))) data-method="DELETE" data-reload="true" data-confirm="Xóa tệp học liệu này?"{button class="button"{"Xóa tệp"}}}}
        form class="stack" data-api=(format!("/api/admin/lessons/{track}/{id}/resources")) data-kind="resource"{label class="field"{span{"Tải tệp bổ sung"}input type="file" data-upload;}div class="uploads"{}(submit("Gắn tệp vào bài học"))}}
        details class="panel article"{summary{"Xóa bài học"}p{"Tiến độ và câu hỏi của bài này cũng bị xóa. Gỡ tệp đính kèm trước khi xóa."}form data-api=(format!("/api/admin/lessons/{track}/{id}")) data-method="DELETE" data-redirect="/quan-tri/hoc-lieu" data-confirm="Xóa bài học cùng tiến độ và câu hỏi liên quan?"{button class="button danger"{"Xóa bài học"}}}}
        },
    ))
}
pub async fn competitions(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Html<String>> {
    if let Err(e) = v.admin() {
        return gate("Quản lý cuộc thi", "/quan-tri/cuoc-thi", &v, e);
    }
    competitions_page(&s, &v, true).await
}
pub async fn public_competitions(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Html<String>> {
    competitions_page(&s, &v, false).await
}
async fn competitions_page(s: &AppState, v: &Viewer, admin: bool) -> Result<Html<String>> {
    let data = api::competitions_data(s, admin).await?;
    Ok(page(
        "Cuộc thi & cơ hội",
        if admin {
            "/quan-tri/cuoc-thi"
        } else {
            "/cuoc-thi"
        },
        v,
        html! {
        (heading("CỘNG ĐỒNG","Cuộc thi & cơ hội","Kiểm tra điều kiện và thời hạn tại nguồn chính thức trước khi đăng ký."))
        @if admin{details class="panel article"{summary{"Thêm cơ hội mới"}form class="stack narrow" data-api="/api/admin/competitions"{(input("Mã (chữ thường, số, dấu -)","id","text","",true))(input("Tên cuộc thi","title","text","",true))(input("Ban tổ chức","organizer","text","",true))(input("Nguồn chính thức (HTTPS)","source_url","url","",true))(input("Hạn đăng ký","deadline","text","",false))(area("Giới thiệu","summary","",true,4))(submit("Lưu cuộc thi"))}}}
        div class="resource-grid"{@for item in &data{article class="panel article"{span class="badge"{(text(item,"organizer"))}h2{(text(item,"title"))}p class="prose"{(text(item,"summary"))}p class="muted"{(text(item,"deadline"))}a href=(text(item,"source_url")) target="_blank" rel="noopener noreferrer"{"Xem thông tin chính thức ↗"}
        @if admin{form data-api=(format!("/api/admin/competitions/{}",text(item,"id"))) data-method="PATCH"{(hidden("revision",item["revision"].as_i64().unwrap_or(1)))input type="hidden" name="hidden" data-type="boolean" value=(if item["hidden"]==true{"false"}else{"true"});button class="button" {(if item["hidden"]==true{"Hiện lại"}else{"Ẩn thông tin"})}}}
        }}}
        @if data.is_empty(){(empty("Chưa có cơ hội được công bố. Hãy quay lại sau."))}
        },
    ))
}
