use crate::{
    AppState, catalog, db,
    error::{AppError, Result},
    security::{self, Viewer},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::{get, patch},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/lab/membership", get(current).post(request))
        .route("/api/admin/lab-memberships", get(list))
        .route("/api/admin/lab-memberships/{id}", patch(decide))
}
pub async fn current_data(s: &AppState, v: &Viewer) -> Result<Value> {
    let u = v.auth()?;
    let c = s.pool.get().await?;
    Ok(
        json!({"active":u.active||u.role=="admin","request":db::value(&c,"SELECT to_jsonb(m)-'user_id'-'reviewed_by' FROM lab_memberships m WHERE user_id=$1",&[&u.id]).await?}),
    )
}
async fn current(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Json<Value>> {
    Ok(Json(current_data(&s, &v).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    tracks: Vec<String>,
    message: String,
}
async fn request(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Json(b): Json<Request>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    let message = security::text(&b.message, 10, 1000, "Mục tiêu học tập")?;
    if b.tracks.is_empty()
        || b.tracks.len() > 3
        || b.tracks
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != b.tracks.len()
    {
        return Err(AppError::bad("Chọn ít nhất một môn học."));
    }
    for track in &b.tracks {
        catalog::valid_track(track)?;
    }
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let row = tx
        .query_one(
            "SELECT phone,blocked,verified FROM users WHERE id=$1 FOR UPDATE",
            &[&u.id],
        )
        .await?;
    if row.get::<_, bool>("blocked") || !row.get::<_, bool>("verified") {
        return Err(AppError::forbidden("Tài khoản chưa đủ điều kiện."));
    }
    if row.get::<_, String>("phone").is_empty() {
        return Err(AppError::bad(
            "Bổ sung số điện thoại trước khi gửi yêu cầu.",
        ));
    }
    let old = tx
        .query_opt(
            "SELECT status FROM lab_memberships WHERE user_id=$1 FOR UPDATE",
            &[&u.id],
        )
        .await?;
    if u.role == "admin"
        || old
            .as_ref()
            .is_some_and(|r| r.get::<_, String>(0) == "approved")
    {
        return Err(AppError::conflict("Bạn đã có quyền học đầy đủ."));
    }
    if old
        .as_ref()
        .is_some_and(|r| r.get::<_, String>(0) == "pending")
    {
        return Ok(Json(
            json!({"ok":true,"duplicate":true,"message":"Yêu cầu đang chờ người chủ trì duyệt."}),
        ));
    }
    tx.execute("INSERT INTO lab_memberships(user_id,status,tracks,message) VALUES($1,'pending',$2,$3) ON CONFLICT(user_id) DO UPDATE SET status='pending',tracks=$2,message=$3,decision_note='',reviewed_by=NULL,reviewed_at=NULL,updated_at=now(),revision=lab_memberships.revision+1",&[&u.id,&b.tracks,&message]).await?;
    db::audit(
        &tx,
        Some(u.id),
        "lab.requested",
        &u.id.to_string(),
        json!({"tracks":b.tracks}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"ok":true,"duplicate":false,"message":"Yêu cầu tham gia Lab đã được lưu.","reload":true}),
    ))
}
#[derive(Deserialize, Default)]
pub struct Filter {
    pub status: Option<String>,
    pub query: Option<String>,
    pub page: Option<i64>,
}
pub async fn list_data(s: &AppState, v: &Viewer, q: Filter) -> Result<Value> {
    v.admin()?;
    let status = q.status.unwrap_or_else(|| "pending".into());
    if !["all", "pending", "approved", "rejected", "revoked"].contains(&status.as_str()) {
        return Err(AppError::bad("Trạng thái không hợp lệ."));
    }
    let query = security::text(q.query.as_deref().unwrap_or(""), 0, 150, "Tìm kiếm")?;
    let page = q.page.unwrap_or(1).clamp(1, 10000);
    let offset = (page - 1) * 30;
    let c = s.pool.get().await?;
    let items=db::values(&c,"SELECT to_jsonb(m)||jsonb_build_object('name',u.name,'email',u.email,'phone',u.phone,'verified',u.verified,'blocked',u.blocked) FROM lab_memberships m JOIN users u ON u.id=m.user_id WHERE ($1='all' OR m.status=$1) AND ($2='' OR position(lower($2) in lower(u.name||' '||u.email||' '||u.phone))>0) ORDER BY m.updated_at DESC,m.user_id LIMIT 30 OFFSET $3",&[&status,&query,&offset]).await?;
    let total:i64=c.query_one("SELECT count(*) FROM lab_memberships m JOIN users u ON u.id=m.user_id WHERE ($1='all' OR m.status=$1) AND ($2='' OR position(lower($2) in lower(u.name||' '||u.email||' '||u.phone))>0)",&[&status,&query]).await?.get(0);
    Ok(json!({"items":items,"total":total,"page":page}))
}
async fn list(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Query(q): Query<Filter>,
) -> Result<Json<Value>> {
    Ok(Json(list_data(&s, &v, q).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    status: String,
    revision: i32,
    #[serde(default)]
    note: String,
}
async fn decide(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Json(b): Json<Decision>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    if !["approved", "rejected", "revoked"].contains(&b.status.as_str()) || b.revision < 1 {
        return Err(AppError::bad("Quyết định không hợp lệ."));
    }
    let mut note = security::text(
        &b.note,
        if b.status == "approved" { 0 } else { 3 },
        1000,
        "Phản hồi",
    )?;
    if note.is_empty() {
        note = "Đã duyệt tham gia Lab và cấp quyền học đầy đủ ba môn.".into();
    }
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let account = tx
        .query_opt(
            "SELECT role,verified,blocked FROM users WHERE id=$1 FOR UPDATE",
            &[&id],
        )
        .await?
        .ok_or_else(|| AppError::missing("Không tìm thấy tài khoản."))?;
    if account.get::<_, String>("role") != "student" {
        return Err(AppError::forbidden("Không thay đổi quyền người chủ trì."));
    }
    let current = tx
        .query_opt(
            "SELECT status,revision FROM lab_memberships WHERE user_id=$1 FOR UPDATE",
            &[&id],
        )
        .await?
        .ok_or_else(|| AppError::missing("Không tìm thấy yêu cầu."))?;
    let previous: String = current.get("status");
    if current.get::<_, i32>("revision") != b.revision
        || previous
            != if b.status == "revoked" {
                "approved"
            } else {
                "pending"
            }
    {
        return Err(AppError::conflict(
            "Yêu cầu đã thay đổi. Tải lại danh sách.",
        ));
    }
    if b.status == "approved"
        && (!account.get::<_, bool>("verified") || account.get::<_, bool>("blocked"))
    {
        return Err(AppError::conflict(
            "Chỉ duyệt tài khoản đã xác minh và không bị khóa.",
        ));
    }
    tx.execute("UPDATE lab_memberships SET status=$2,decision_note=$3,reviewed_by=$4,reviewed_at=now(),updated_at=now(),revision=revision+1 WHERE user_id=$1",&[&id,&b.status,&note,&u.id]).await?;
    db::audit(
        &tx,
        Some(u.id),
        &format!("lab.{}", b.status),
        &id.to_string(),
        json!({"previousStatus":previous,"note":note,"revision":b.revision+1}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"ok":true,"message":if b.status=="approved"{"Đã duyệt và cấp quyền học đầy đủ."}else if b.status=="revoked"{"Đã thu hồi quyền Lab."}else{"Đã lưu phản hồi chưa duyệt."},"reload":true}),
    ))
}
