use crate::{
    AppState, db,
    error::{AppError, Result},
    practice,
    security::{self, Viewer},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/conversations/{track}", get(list).post(create))
        .route("/api/conversation/{id}", get(detail).patch(close))
        .route("/api/conversation/{id}/messages", post(reply))
}
pub async fn list_data(s: &AppState, v: &Viewer, track: &str) -> Result<Vec<Value>> {
    practice::access(s, v, track).await?;
    let u = v.auth()?;
    let c = s.pool.get().await?;
    db::values(&c,"SELECT to_jsonb(c)||jsonb_build_object('student_name',u.name,'student_email',u.email,'messages',(SELECT count(*) FROM messages m WHERE m.conversation_id=c.id)) FROM conversations c JOIN users u ON u.id=c.user_id WHERE track=$1 AND (user_id=$2 OR $3) ORDER BY updated_at DESC LIMIT 100",&[&track,&u.id,&(u.role=="admin")]).await
}
async fn list(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Json<Value>> {
    Ok(Json(json!(list_data(&s, &v, &track).await?)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Create {
    subject: String,
    body: String,
    exercise_id: Option<String>,
}
async fn create(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
    Json(b): Json<Create>,
) -> Result<Json<Value>> {
    practice::access(&s, &v, &track).await?;
    let u = v.auth()?;
    let subject = security::text(&b.subject, 3, 160, "Chủ đề")?;
    let body = security::text(&b.body, 3, 3000, "Nội dung")?;
    if let Some(id) = &b.exercise_id {
        practice::exercise_data(&s, &v, &track, id).await?;
    }
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let id:Uuid=tx.query_one("INSERT INTO conversations(user_id,track,exercise_id,subject) VALUES($1,$2,$3,$4) RETURNING id",&[&u.id,&track,&b.exercise_id,&subject]).await?.get(0);
    tx.execute(
        "INSERT INTO messages(conversation_id,author_id,body,request_id) VALUES($1,$2,$3,$4)",
        &[&id, &u.id, &body, &Uuid::new_v4()],
    )
    .await?;
    db::audit(
        &tx,
        Some(u.id),
        "conversation.created",
        &id.to_string(),
        json!({"track":track}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":id,"message":"Đã tạo trao đổi.","redirect":format!("/trao-doi/chu-de/{id}")}),
    ))
}
pub async fn detail_data(s: &AppState, v: &Viewer, id: Uuid) -> Result<Value> {
    let u = v.auth()?;
    let c = s.pool.get().await?;
    let mut body=db::value(&c,"SELECT to_jsonb(c)||jsonb_build_object('student_name',u.name) FROM conversations c JOIN users u ON u.id=c.user_id WHERE c.id=$1 AND (c.user_id=$2 OR $3)",&[&id,&u.id,&(u.role=="admin")]).await?;
    if body.is_null() {
        return Err(AppError::missing("Không tìm thấy trao đổi."));
    }
    practice::access(s, v, body["track"].as_str().unwrap()).await?;
    body["messages"]=json!(db::values(&c,"SELECT jsonb_build_object('id',m.id,'body',m.body,'created_at',m.created_at,'author',u.name,'role',u.role) FROM messages m JOIN users u ON u.id=m.author_id WHERE conversation_id=$1 ORDER BY m.created_at,m.id",&[&id]).await?);
    Ok(body)
}
async fn detail(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(Json(detail_data(&s, &v, id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Reply {
    body: String,
    request_id: Uuid,
}
async fn reply(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Json(b): Json<Reply>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    let info = detail_data(&s, &v, id).await?;
    let body = security::text(&b.body, 3, 3000, "Tin nhắn")?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let closed: bool = tx
        .query_one(
            "SELECT closed FROM conversations WHERE id=$1 FOR UPDATE",
            &[&id],
        )
        .await?
        .get(0);
    if closed {
        return Err(AppError::conflict(
            "Trao đổi đã đóng. Mở lại trước khi trả lời.",
        ));
    }
    if let Some(old) = tx
        .query_opt(
            "SELECT conversation_id,body FROM messages WHERE author_id=$1 AND request_id=$2",
            &[&u.id, &b.request_id],
        )
        .await?
    {
        if old.get::<_, Uuid>(0) != id || old.get::<_, String>(1) != body {
            return Err(AppError::conflict("Mã gửi đã dùng cho nội dung khác."));
        }
        return Ok(Json(
            json!({"duplicate":true,"message":"Tin nhắn đã được gửi.","reload":true}),
        ));
    }
    tx.execute(
        "INSERT INTO messages(conversation_id,author_id,body,request_id) VALUES($1,$2,$3,$4)",
        &[&id, &u.id, &body, &b.request_id],
    )
    .await?;
    tx.execute(
        "UPDATE conversations SET updated_at=now(),revision=revision+1 WHERE id=$1",
        &[&id],
    )
    .await?;
    db::audit(
        &tx,
        Some(u.id),
        "conversation.replied",
        &id.to_string(),
        json!({"track":info["track"]}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"message":"Đã gửi tin nhắn.","reload":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Close {
    closed: bool,
    revision: i32,
}
async fn close(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Json(b): Json<Close>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    detail_data(&s, &v, id).await?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    if tx.execute("UPDATE conversations SET closed=$2,revision=revision+1,updated_at=now() WHERE id=$1 AND revision=$3",&[&id,&b.closed,&b.revision]).await?!=1{return Err(AppError::conflict("Trao đổi đã thay đổi. Tải lại."));}
    db::audit(
        &tx,
        Some(u.id),
        "conversation.closed",
        &id.to_string(),
        json!({"closed":b.closed}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã cập nhật trạng thái trao đổi.","reload":true}),
    ))
}
