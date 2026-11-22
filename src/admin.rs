use crate::{
    AppState, auth, catalog, db,
    error::{AppError, Result},
    mail,
    security::{self, Viewer},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::{get, patch, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/admin/overview", get(overview))
        .route("/api/admin/accounts", get(accounts).post(invite))
        .route("/api/admin/accounts/{id}", get(account))
        .route("/api/admin/accounts/{id}/history", get(history))
        .route("/api/admin/accounts/{id}/notes", post(note))
        .route("/api/admin/accounts/{id}/email", post(email))
        .route("/api/admin/accounts/{id}/revoke-sessions", post(revoke))
        .route("/api/admin/users/{id}", patch(block))
        .route("/api/admin/audit", get(audit))
        .route("/api/admin/courses", get(courses))
        .route("/api/admin/courses/{track}", put(update_course))
        .route("/api/admin/lessons", get(lessons).post(create_lesson))
        .route(
            "/api/admin/lessons/{track}/{id}",
            put(update_lesson).delete(delete_lesson),
        )
        .route("/api/admin/classroom/progress", get(progress_report))
        .route("/api/competitions", get(public_competitions))
        .route(
            "/api/admin/competitions",
            get(competitions).post(create_competition),
        )
        .route("/api/admin/competitions/{id}", patch(toggle_competition))
}
pub async fn overview_data(s: &AppState, v: &Viewer) -> Result<Value> {
    v.admin()?;
    let c = s.pool.get().await?;
    db::value(&c,"SELECT jsonb_build_object('students',(SELECT count(*) FROM users WHERE role='student'),'pending',(SELECT count(*) FROM lab_memberships WHERE status='pending'),'members',(SELECT count(*) FROM lab_memberships WHERE status='approved'),'lessons',(SELECT count(*) FROM lessons WHERE status='published'),'submissions',(SELECT count(*) FROM submissions WHERE status='pending_manual'),'conversations',(SELECT count(*) FROM conversations WHERE NOT closed))",&[]).await
}
async fn overview(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Json<Value>> {
    Ok(Json(overview_data(&s, &v).await?))
}
#[derive(Deserialize, Default)]
pub struct Search {
    pub q: Option<String>,
    pub state: Option<String>,
    pub page: Option<i64>,
}
pub async fn accounts_data(s: &AppState, v: &Viewer, q: Search) -> Result<Value> {
    v.admin()?;
    let query = security::text(q.q.as_deref().unwrap_or(""), 0, 200, "Tìm kiếm")?;
    let page = q.page.unwrap_or(1).clamp(1, 10000);
    let filter = q.state.unwrap_or_else(|| "all".into());
    if !["all", "blocked", "active", "unverified", "invited"].contains(&filter.as_str()) {
        return Err(AppError::bad("Bộ lọc không hợp lệ."));
    }
    let c = s.pool.get().await?;
    let items=db::values(&c,"SELECT to_jsonb(u)-'password_hash'||jsonb_build_object('active',EXISTS(SELECT 1 FROM lab_memberships m WHERE m.user_id=u.id AND status='approved') OR u.role='admin') FROM users u WHERE ($1='' OR position(lower($1) in lower(name||' '||email||' '||phone))>0) AND ($2='all' OR $2='blocked' AND blocked OR $2='unverified' AND NOT verified OR $2='invited' AND setup_pending OR $2='active' AND NOT blocked AND verified AND (role='admin' OR EXISTS(SELECT 1 FROM lab_memberships m WHERE m.user_id=u.id AND status='approved'))) ORDER BY created_at DESC,id LIMIT 25 OFFSET $3",&[&query,&filter,&((page-1)*25)]).await?;
    let total:i64=c.query_one("SELECT count(*) FROM users u WHERE ($1='' OR position(lower($1) in lower(name||' '||email||' '||phone))>0) AND ($2='all' OR $2='blocked' AND blocked OR $2='unverified' AND NOT verified OR $2='invited' AND setup_pending OR $2='active' AND NOT blocked AND verified AND (role='admin' OR EXISTS(SELECT 1 FROM lab_memberships m WHERE m.user_id=u.id AND status='approved')))",&[&query,&filter]).await?.get(0);
    Ok(json!({"items":items,"total":total,"page":page}))
}
async fn accounts(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Query(q): Query<Search>,
) -> Result<Json<Value>> {
    Ok(Json(accounts_data(&s, &v, q).await?))
}
pub async fn account_data(s: &AppState, v: &Viewer, id: Uuid) -> Result<Value> {
    v.admin()?;
    let c = s.pool.get().await?;
    let user=db::value(&c,"SELECT to_jsonb(u)-'password_hash'||jsonb_build_object('active',EXISTS(SELECT 1 FROM lab_memberships m WHERE user_id=u.id AND status='approved') OR role='admin') FROM users u WHERE id=$1",&[&id]).await?;
    if user.is_null() {
        return Err(AppError::missing("Không tìm thấy tài khoản."));
    }
    Ok(json!({"user":user,
        "sessions":db::values(&c,"SELECT jsonb_build_object('user_agent',user_agent,'created_at',created_at,'expires_at',expires_at) FROM sessions WHERE user_id=$1 AND expires_at>now()",&[&id]).await?,
        "membership":db::value(&c,"SELECT to_jsonb(m) FROM lab_memberships m WHERE user_id=$1",&[&id]).await?,
        "progress":db::values(&c,"SELECT jsonb_build_object('track',track,'completed',count(*) FILTER(WHERE completed),'started',count(*)) FROM classroom_progress WHERE user_id=$1 GROUP BY track",&[&id]).await?,
        "history":db::values(&c,"SELECT to_jsonb(a)||jsonb_build_object('actor_name',u.name) FROM audit_logs a LEFT JOIN users u ON u.id=a.actor_id WHERE target=$1 OR actor_id=$2 ORDER BY a.id DESC LIMIT 50",&[&id.to_string(),&id]).await?}))
}
async fn account(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(Json(account_data(&s, &v, id).await?))
}
async fn history(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Query(q): Query<Search>,
) -> Result<Json<Value>> {
    v.admin()?;
    let c = s.pool.get().await?;
    Ok(Json(
        json!({"items":db::values(&c,"SELECT to_jsonb(a)||jsonb_build_object('actor_name',u.name) FROM audit_logs a LEFT JOIN users u ON u.id=a.actor_id WHERE target=$1 OR actor_id=$2 ORDER BY a.id DESC LIMIT 50 OFFSET $3",&[&id.to_string(),&id,&((q.page.unwrap_or(1).clamp(1,10000)-1)*50)]).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Invite {
    name: String,
    email: String,
    phone: String,
    #[serde(default)]
    note: String,
}
async fn invite(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Json(b): Json<Invite>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    let name = security::text(&b.name, 2, 80, "Họ tên")?;
    let email = security::email(&b.email)?;
    let phone = security::phone(&b.phone)?;
    let note = security::text(&b.note, 0, 1000, "Ghi chú")?;
    auth::rate(&s, &format!("invite:{}", u.id), 15).await?;
    let _permit = s.hashes.acquire().await.unwrap();
    let hashed = auth::hash_password(security::token()).await?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let id:Uuid=tx.query_one("INSERT INTO users(name,email,phone,password_hash,setup_pending) VALUES($1,$2,$3,$4,true) RETURNING id",&[&name,&email,&phone,&hashed]).await?.get(0);
    db::audit(
        &tx,
        Some(u.id),
        "user.created",
        &id.to_string(),
        json!({"note":if note.is_empty(){"Tạo hồ sơ và mời tham gia Lab."}else{&note}}),
    )
    .await?;
    tx.commit().await?;
    drop(c);
    let sent = mail::send_token(&s, id, &email, "invite").await.is_ok();
    Ok(Json(
        json!({"id":id,"invitationSent":sent,"message":if sent{"Đã tạo hồ sơ và gửi thư mời."}else{"Đã tạo hồ sơ, nhưng chưa gửi được thư. Hãy dùng gửi lại thư mời."},"redirect":format!("/quan-tri/tai-khoan/{id}")}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Note {
    note: String,
}
async fn note(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Json(b): Json<Note>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    let note = security::text(&b.note, 5, 1000, "Ghi chú")?;
    let c = s.pool.get().await?;
    if c.query_opt("SELECT id FROM users WHERE id=$1", &[&id])
        .await?
        .is_none()
    {
        return Err(AppError::missing("Không tìm thấy tài khoản."));
    }
    db::audit(
        &c,
        Some(u.id),
        "user.note",
        &id.to_string(),
        json!({"note":note}),
    )
    .await?;
    Ok(Json(json!({"message":"Đã lưu ghi chú.","reload":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Block {
    blocked: bool,
    #[serde(default)]
    reason: String,
}
async fn block(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Json(b): Json<Block>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    let reason = security::text(&b.reason, if b.blocked { 5 } else { 0 }, 1000, "Lý do")?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let row = tx
        .query_opt(
            "SELECT role,blocked FROM users WHERE id=$1 FOR UPDATE",
            &[&id],
        )
        .await?
        .ok_or_else(|| AppError::missing("Không tìm thấy tài khoản."))?;
    if row.get::<_, String>("role") == "admin" {
        return Err(AppError::forbidden("Không khóa tài khoản người chủ trì."));
    }
    if row.get::<_, bool>("blocked") != b.blocked {
        tx.execute(
            "UPDATE users SET blocked=$2 WHERE id=$1",
            &[&id, &b.blocked],
        )
        .await?;
        tx.execute("DELETE FROM sessions WHERE user_id=$1", &[&id])
            .await?;
        db::audit(
            &tx,
            Some(u.id),
            if b.blocked {
                "user.blocked"
            } else {
                "user.unblocked"
            },
            &id.to_string(),
            json!({"reason":if reason.is_empty(){"Mở khóa từ hồ sơ quản lý."}else{&reason}}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã cập nhật trạng thái tài khoản.","reload":true}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reason {
    reason: String,
}
async fn revoke(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Json(b): Json<Reason>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    let reason = security::text(&b.reason, 5, 1000, "Lý do")?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let row = tx
        .query_opt("SELECT role FROM users WHERE id=$1 FOR UPDATE", &[&id])
        .await?
        .ok_or_else(|| AppError::missing("Không tìm thấy tài khoản."))?;
    if row.get::<_, String>(0) == "admin" {
        return Err(AppError::forbidden(
            "Người chủ trì tự quản lý phiên của mình.",
        ));
    }
    let count = tx
        .execute("DELETE FROM sessions WHERE user_id=$1", &[&id])
        .await?;
    db::audit(
        &tx,
        Some(u.id),
        "sessions.revoked",
        &id.to_string(),
        json!({"reason":reason,"count":count}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã đăng xuất các phiên hiện có.","reload":true}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Purpose {
    purpose: String,
}
async fn email(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Json(b): Json<Purpose>,
) -> Result<Json<Value>> {
    v.admin()?;
    auth::rate(&s, &format!("admin-email:{id}"), 5).await?;
    let c = s.pool.get().await?;
    let r = c
        .query_opt(
            "SELECT email,role,verified,blocked,setup_pending FROM users WHERE id=$1",
            &[&id],
        )
        .await?
        .ok_or_else(|| AppError::missing("Không tìm thấy tài khoản."))?;
    let expected = if r.get::<_, bool>("setup_pending") {
        "invite"
    } else if r.get::<_, bool>("verified") {
        "reset"
    } else {
        "verify"
    };
    if r.get::<_, String>("role") != "student"
        || r.get::<_, bool>("blocked")
        || b.purpose != expected
    {
        return Err(AppError::conflict(
            "Trạng thái tài khoản đã thay đổi. Tải lại hồ sơ.",
        ));
    }
    drop(c);
    mail::send_token(&s, id, &r.get::<_, String>("email"), &b.purpose).await?;
    Ok(Json(
        json!({"message":"Đã gửi email. Liên kết hết hạn sau 30 phút."}),
    ))
}
pub async fn audit_data(s: &AppState, v: &Viewer, page: i64) -> Result<Vec<Value>> {
    v.admin()?;
    let c = s.pool.get().await?;
    db::values(&c,"SELECT to_jsonb(a)||jsonb_build_object('actor_name',u.name) FROM audit_logs a LEFT JOIN users u ON u.id=a.actor_id ORDER BY a.id DESC LIMIT 50 OFFSET $1",&[&((page.clamp(1,10000)-1)*50)]).await
}
async fn audit(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Query(q): Query<Search>,
) -> Result<Json<Value>> {
    Ok(Json(json!(audit_data(&s, &v, q.page.unwrap_or(1)).await?)))
}
pub async fn courses_data(s: &AppState, v: &Viewer) -> Result<Vec<Value>> {
    v.admin()?;
    let c = s.pool.get().await?;
    db::values(
        &c,
        "SELECT to_jsonb(c) FROM courses c ORDER BY position",
        &[],
    )
    .await
}
async fn courses(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Json<Value>> {
    Ok(Json(json!(courses_data(&s, &v).await?)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CourseEdit {
    title: String,
    description: String,
    status: String,
    revision: i32,
}
async fn update_course(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
    Json(b): Json<CourseEdit>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    catalog::valid_track(&track)?;
    let title = security::text(&b.title, 3, 180, "Tiêu đề")?;
    let description = security::text(&b.description, 5, 3000, "Mô tả")?;
    if !["draft", "published"].contains(&b.status.as_str()) {
        return Err(AppError::bad("Trạng thái không hợp lệ."));
    }
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    if tx.execute("UPDATE courses SET title=$2,description=$3,status=$4,revision=revision+1 WHERE category=$1 AND revision=$5",&[&track,&title,&description,&b.status,&b.revision]).await?!=1{return Err(AppError::conflict("Môn học đã thay đổi. Tải lại trước khi lưu."));}
    db::audit(
        &tx,
        Some(u.id),
        "course.updated",
        &track,
        json!({"status":b.status}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"message":"Đã lưu môn học.","reload":true})))
}
pub async fn lessons_data(s: &AppState, v: &Viewer) -> Result<Vec<Value>> {
    v.admin()?;
    let c = s.pool.get().await?;
    db::values(
        &c,
        "SELECT to_jsonb(l) FROM lessons l ORDER BY track,position",
        &[],
    )
    .await
}
async fn lessons(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Json<Value>> {
    Ok(Json(json!(lessons_data(&s, &v).await?)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LessonEdit {
    track: String,
    id: String,
    body: Value,
    status: String,
    #[serde(default)]
    revision: i32,
}
fn validate_lesson(b: &LessonEdit) -> Result<()> {
    catalog::valid_track(&b.track)?;
    if b.id.is_empty()
        || b.id.len() > 80
        || !b.id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(AppError::bad("Mã bài chỉ gồm chữ, số và dấu gạch ngang."));
    }
    if !["draft", "published"].contains(&b.status.as_str())
        || !b.body.is_object()
        || b.body.to_string().len() > 100_000
    {
        return Err(AppError::bad("Nội dung không hợp lệ."));
    }
    security::text(b.body["title"].as_str().unwrap_or(""), 3, 180, "Tiêu đề")?;
    if b.body["id"] != b.id
        || !b.body["preview"].is_boolean()
        || !b.body["module"].is_u64()
        || !b.body["concepts"].is_array()
        || !b.body["steps"].is_array()
        || !b.body["objectives"].is_array()
    {
        return Err(AppError::bad(
            "Thiếu cấu trúc bài học: id, module, preview, concepts, steps, objectives.",
        ));
    }
    let q = &b.body["quiz"];
    if b.body["minutes"]
        .as_u64()
        .is_none_or(|n| n == 0 || n > 1440)
        || b.body["module"].as_u64().is_none_or(|n| n == 0 || n > 1000)
    {
        return Err(AppError::bad("Học phần hoặc thời lượng không hợp lệ."));
    }
    for key in ["steps", "objectives"] {
        let items = b.body[key].as_array().unwrap();
        if items.is_empty()
            || items.len() > 50
            || items.iter().any(|item| {
                item.as_str()
                    .is_none_or(|s| s.trim().is_empty() || s.chars().count() > 3000)
            })
        {
            return Err(AppError::bad(
                "Mục tiêu và bước thực hành cần 1–50 nội dung rõ ràng.",
            ));
        }
    }
    let concepts = b.body["concepts"].as_array().unwrap();
    if concepts.is_empty()
        || concepts.len() > 50
        || concepts.iter().any(|item| {
            item.as_array().is_none_or(|pair| {
                pair.len() != 2
                    || pair.iter().any(|v| {
                        v.as_str()
                            .is_none_or(|s| s.trim().is_empty() || s.chars().count() > 10000)
                    })
            })
        })
    {
        return Err(AppError::bad("Mỗi khái niệm cần tên và phần giải thích."));
    }
    let options = q["options"]
        .as_array()
        .ok_or_else(|| AppError::bad("Cần các lựa chọn tự kiểm."))?;
    if !(2..=8).contains(&options.len())
        || options.iter().any(|x| !x.is_string())
        || q["answer"]
            .as_u64()
            .is_none_or(|i| i >= options.len() as u64)
        || q["question"].as_str().is_none_or(|q| q.trim().is_empty())
    {
        return Err(AppError::bad("Câu hỏi tự kiểm không hợp lệ."));
    }
    if let Some(urls) = b.body["sources"].as_array() {
        for url in urls {
            safe_url(url.as_str().unwrap_or(""))?;
        }
    }
    Ok(())
}
async fn create_lesson(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Json(b): Json<LessonEdit>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    validate_lesson(&b)?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    tx.execute(
        "INSERT INTO lessons(track,id,body,status,position) VALUES($1,$2,$3,$4,9999)",
        &[&b.track, &b.id, &b.body, &b.status],
    )
    .await?;
    db::audit(
        &tx,
        Some(u.id),
        "lesson.created",
        &format!("{}/{}", b.track, b.id),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã tạo bài học.","redirect":"/quan-tri/hoc-lieu"}),
    ))
}
async fn update_lesson(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
    Json(b): Json<LessonEdit>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    validate_lesson(&b)?;
    if track != b.track || id != b.id {
        return Err(AppError::bad("Không thay đổi mã môn hoặc mã bài khi sửa."));
    }
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    if tx.execute("UPDATE lessons SET body=$3,status=$4,revision=revision+1 WHERE track=$1 AND id=$2 AND revision=$5",&[&track,&id,&b.body,&b.status,&b.revision]).await?!=1{return Err(AppError::conflict("Bài học đã thay đổi. Tải lại trước khi lưu."));}
    db::audit(
        &tx,
        Some(u.id),
        "lesson.updated",
        &format!("{track}/{id}"),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"message":"Đã lưu nội dung.","reload":true})))
}
async fn delete_lesson(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let n = tx
        .execute(
            "DELETE FROM lessons WHERE track=$1 AND id=$2",
            &[&track, &id],
        )
        .await?;
    if n == 0 {
        return Err(AppError::missing("Không tìm thấy bài học."));
    }
    db::audit(
        &tx,
        Some(u.id),
        "lesson.deleted",
        &format!("{track}/{id}"),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"message":"Đã xóa bài học.","reload":true})))
}
async fn progress_report(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Json<Value>> {
    v.admin()?;
    let c = s.pool.get().await?;
    Ok(Json(json!(db::values(&c,"SELECT jsonb_build_object('name',u.name,'email',u.email,'track',p.track,'started',count(*),'completed',count(*) FILTER(WHERE completed)) FROM classroom_progress p JOIN users u ON u.id=p.user_id GROUP BY u.id,p.track ORDER BY u.name",&[]).await?)))
}
fn safe_url(s: &str) -> Result<()> {
    let u = url::Url::parse(s).map_err(|_| AppError::bad("URL không hợp lệ."))?;
    if u.scheme() != "https" || !u.username().is_empty() || u.password().is_some() {
        return Err(AppError::bad("Nguồn tham khảo phải dùng HTTPS."));
    }
    Ok(())
}
pub async fn competitions_data(s: &AppState, admin: bool) -> Result<Vec<Value>> {
    let c = s.pool.get().await?;
    db::values(&c,"SELECT body||jsonb_build_object('id',id,'hidden',hidden,'revision',revision) FROM competitions WHERE NOT hidden OR $1 ORDER BY updated_at DESC",&[&admin]).await
}
async fn public_competitions(State(s): State<AppState>) -> Result<Json<Value>> {
    Ok(Json(json!(competitions_data(&s, false).await?)))
}
async fn competitions(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Json<Value>> {
    v.admin()?;
    Ok(Json(json!(competitions_data(&s, true).await?)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Competition {
    id: String,
    title: String,
    organizer: String,
    summary: String,
    source_url: String,
    deadline: String,
}
async fn create_competition(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Json(b): Json<Competition>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    if b.id.is_empty()
        || b.id.len() > 80
        || !b
            .id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err(AppError::bad("Mã cuộc thi không hợp lệ."));
    }
    let title = security::text(&b.title, 3, 160, "Tiêu đề")?;
    let organizer = security::text(&b.organizer, 2, 160, "Ban tổ chức")?;
    let summary = security::text(&b.summary, 5, 1000, "Mô tả")?;
    safe_url(&b.source_url)?;
    let deadline = security::text(&b.deadline, 0, 200, "Hạn đăng ký")?;
    let body = json!({"title":title,"organizer":organizer,"summary":summary,"source_url":b.source_url,"deadline":deadline});
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    tx.execute(
        "INSERT INTO competitions(id,body) VALUES($1,$2)",
        &[&b.id, &body],
    )
    .await?;
    db::audit(&tx, Some(u.id), "competition.created", &b.id, json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({"message":"Đã thêm cuộc thi.","reload":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Toggle {
    hidden: bool,
    revision: i32,
}
async fn toggle_competition(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<String>,
    Json(b): Json<Toggle>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    if tx.execute("UPDATE competitions SET hidden=$2,revision=revision+1,updated_at=now() WHERE id=$1 AND revision=$3",&[&id,&b.hidden,&b.revision]).await?!=1{return Err(AppError::conflict("Cuộc thi đã thay đổi. Tải lại danh sách."));}
    db::audit(
        &tx,
        Some(u.id),
        "competition.visibility",
        &id,
        json!({"hidden":b.hidden}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã cập nhật cuộc thi.","reload":true}),
    ))
}
