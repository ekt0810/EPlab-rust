use crate::{
    AppState, catalog, db,
    error::{AppError, Result},
    security::{self, Viewer},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/practice/{track}/exercises", get(exercises))
        .route("/api/practice/{track}/exercises/{id}", get(exercise))
        .route("/api/practice/{track}/assignments", get(assignments))
        .route(
            "/api/practice/{track}/submissions",
            get(submissions).post(submit),
        )
        .route("/api/practice/submissions/{id}", get(detail))
        .route(
            "/api/admin/practice/{track}/assignments",
            get(admin_assignments).post(assign),
        )
        .route(
            "/api/admin/practice/assignments/{id}",
            patch(close_assignment),
        )
        .route(
            "/api/admin/practice/{track}/submissions",
            get(admin_submissions),
        )
        .route("/api/admin/practice/submissions/{id}/review", post(review))
}
pub async fn access(s: &AppState, v: &Viewer, track: &str) -> Result<()> {
    v.learn()?;
    catalog::valid_track(track)?;
    let c = s.pool.get().await?;
    let status: String = c
        .query_one("SELECT status FROM courses WHERE category=$1", &[&track])
        .await?
        .get(0);
    if status != "published" && v.admin().is_err() {
        return Err(AppError::forbidden("Môn học chưa được mở."));
    }
    Ok(())
}
pub async fn exercise_data(s: &AppState, v: &Viewer, track: &str, id: &str) -> Result<Value> {
    access(s, v, track).await?;
    let c = s.pool.get().await?;
    let mut body: Value = c
        .query_opt(
            "SELECT body FROM exercises WHERE id=$1 AND track=$2",
            &[&id, &track],
        )
        .await?
        .ok_or_else(|| AppError::missing("Không tìm thấy bài thực hành."))?
        .get(0);
    if !body["checks"].is_array() {
        body["checks"] = json!(
            body["criteria"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| json!([
                    format!("Đối chiếu {}", c["id"].as_str().unwrap_or("")),
                    c["text"]
                ]))
                .collect::<Vec<_>>()
        );
    }
    Ok(body)
}
pub async fn exercises_data(s: &AppState, v: &Viewer, track: &str) -> Result<Vec<Value>> {
    catalog::valid_track(track)?;
    let c = s.pool.get().await?;
    let user = v.0.as_ref().map(|u| u.id);
    db::values(&c,"SELECT jsonb_build_object('id',e.id,'track',e.track,'title',body->>'title','brief',body->>'brief','level',body->>'level','difficulty',body->>'difficulty','minutes',body->'minutes','order',body->'order','completed',EXISTS(SELECT 1 FROM submissions s WHERE s.exercise_id=e.id AND s.user_id=$2 AND s.status='passed')) FROM exercises e WHERE track=$1 ORDER BY id",&[&track,&user]).await
}
async fn exercises(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Json<Value>> {
    Ok(Json(json!(exercises_data(&s, &v, &track).await?)))
}
async fn exercise(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
) -> Result<Json<Value>> {
    Ok(Json(exercise_data(&s, &v, &track, &id).await?))
}
pub async fn assignments_data(
    s: &AppState,
    v: &Viewer,
    track: &str,
    admin: bool,
) -> Result<Vec<Value>> {
    access(s, v, track).await?;
    let u = if admin { v.admin()? } else { v.auth()? };
    let c = s.pool.get().await?;
    db::values(&c,"SELECT to_jsonb(a)||jsonb_build_object('title',e.body->>'title','track',e.track,'student_name',u.name) FROM assignments a JOIN exercises e ON e.id=a.exercise_id LEFT JOIN users u ON u.id=a.student_id WHERE e.track=$1 AND ($3 OR student_id IS NULL OR student_id=$2) ORDER BY a.created_at DESC LIMIT 100",&[&track,&u.id,&admin]).await
}
async fn assignments(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Json<Value>> {
    Ok(Json(json!(assignments_data(&s, &v, &track, false).await?)))
}
async fn admin_assignments(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Json<Value>> {
    Ok(Json(json!(assignments_data(&s, &v, &track, true).await?)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Assignment {
    exercise_id: String,
    student_id: Option<Uuid>,
    #[serde(default)]
    instructions: String,
    due_at: Option<chrono::DateTime<chrono::Utc>>,
}
async fn assign(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
    Json(b): Json<Assignment>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    exercise_data(&s, &v, &track, &b.exercise_id).await?;
    let instructions = security::text(&b.instructions, 0, 3000, "Yêu cầu")?;
    if b.due_at.is_some_and(|d| d <= chrono::Utc::now()) {
        return Err(AppError::bad("Hạn nộp phải ở tương lai."));
    }
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    if let Some(id) = b.student_id
        && tx
            .query_opt(
                "SELECT id FROM users WHERE id=$1 AND role='student' AND NOT blocked",
                &[&id],
            )
            .await?
            .is_none()
    {
        return Err(AppError::bad("Người nhận không hợp lệ."));
    }
    let id:Uuid=tx.query_one("INSERT INTO assignments(exercise_id,student_id,assigned_by,instructions,due_at) VALUES($1,$2,$3,$4,$5) RETURNING id",&[&b.exercise_id,&b.student_id,&u.id,&instructions,&b.due_at]).await?.get(0);
    db::audit(
        &tx,
        Some(u.id),
        "assignment.created",
        &id.to_string(),
        json!({"track":track,"exercise":b.exercise_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":id,"message":"Đã giao bài thực hành.","reload":true}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Closed {
    closed: bool,
}
async fn close_assignment(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Json(b): Json<Closed>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    if tx
        .execute(
            "UPDATE assignments SET closed=$2 WHERE id=$1",
            &[&id, &b.closed],
        )
        .await?
        == 0
    {
        return Err(AppError::missing("Không tìm thấy lần giao bài."));
    }
    db::audit(
        &tx,
        Some(u.id),
        "assignment.closed",
        &id.to_string(),
        json!({"closed":b.closed}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã cập nhật lần giao bài.","reload":true}),
    ))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SelfCheck {
    index: usize,
    status: String,
    observed: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Submission {
    exercise_id: String,
    assignment_id: Option<Uuid>,
    notes: String,
    #[serde(default)]
    self_checks: Vec<SelfCheck>,
    file_ids: Vec<Uuid>,
}
async fn submit(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
    Json(b): Json<Submission>,
) -> Result<Json<Value>> {
    let u = v.learn()?;
    let exercise = exercise_data(&s, &v, &track, &b.exercise_id).await?;
    let notes = security::text(&b.notes, 15, 3000, "Ghi chú bài làm")?;
    if b.file_ids.is_empty()
        || b.file_ids.len() > 8
        || b.file_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != b.file_ids.len()
    {
        return Err(AppError::bad("Nộp 1–8 tệp khác nhau."));
    }
    let n = exercise["checks"].as_array().unwrap().len();
    if b.self_checks.len() > 20
        || b.self_checks
            .iter()
            .map(|c| c.index)
            .collect::<std::collections::HashSet<_>>()
            .len()
            != b.self_checks.len()
        || b.self_checks.iter().any(|c| {
            c.index >= n
                || !["not_run", "pass", "fail"].contains(&c.status.as_str())
                || c.observed.chars().count() > 600
                || (c.status != "not_run" && c.observed.trim().is_empty())
        })
    {
        return Err(AppError::bad(
            "Kết quả tự kiểm thiếu quan sát hoặc không thuộc đề bài.",
        ));
    }
    let mut ids = b.file_ids.clone();
    ids.sort();
    let fingerprint = security::hash(
        &json!([b.exercise_id, b.assignment_id, notes, b.self_checks, ids]).to_string(),
    );
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    tx.query_one("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&u.id])
        .await?;
    if let Some(r) = tx
        .query_opt(
            "SELECT id FROM submissions WHERE user_id=$1 AND fingerprint=$2",
            &[&u.id, &fingerprint],
        )
        .await?
    {
        let id: Uuid = r.get(0);
        return Ok(Json(
            json!({"id":id,"duplicate":true,"redirect":format!("/bai-nop/{id}")}),
        ));
    }
    if let Some(id) = b.assignment_id {
        let row = tx
            .query_opt(
                "SELECT student_id,exercise_id,closed FROM assignments WHERE id=$1 FOR UPDATE",
                &[&id],
            )
            .await?
            .ok_or_else(|| AppError::bad("Lần giao bài không tồn tại."))?;
        if row.get::<_, String>("exercise_id") != b.exercise_id
            || row.get::<_, bool>("closed")
            || row
                .get::<_, Option<Uuid>>("student_id")
                .is_some_and(|id| id != u.id)
        {
            return Err(AppError::forbidden(
                "Lần giao bài đã đóng hoặc không dành cho bạn.",
            ));
        }
    }
    let rows=tx.query("SELECT id,name,content_type,size FROM files WHERE id=ANY($1) AND owner_id=$2 AND submission_id IS NULL AND lesson_id IS NULL FOR UPDATE",&[&b.file_ids,&u.id]).await?;
    if rows.len() != b.file_ids.len() {
        return Err(AppError::bad("Tệp không thuộc tài khoản hoặc đã được nộp."));
    }
    let images = rows
        .iter()
        .filter(|r| r.get::<_, String>("content_type").starts_with("image/"))
        .count();
    if !(1..=2).contains(&images)
        || rows.len() - images > 6
        || rows.iter().map(|r| r.get::<_, i64>("size")).sum::<i64>() > 25 * 1024 * 1024
    {
        return Err(AppError::bad(
            "Cần 1–2 ảnh bằng chứng, tối đa 6 tệp nguồn và tổng 25 MB.",
        ));
    }
    for r in &rows {
        let name = r.get::<_, String>("name").to_lowercase();
        let ext = name.rsplit('.').next().unwrap_or("");
        if ["c", "h"].contains(&ext) && track != "embedded"
            || ["sldprt", "sldasm", "slddrw", "step", "stp", "stl"].contains(&ext)
                && track != "solidworks"
            || ext.starts_with("kicad") && track != "kicad"
        {
            return Err(AppError::bad("Tệp nguồn không đúng môn học."));
        }
    }
    let id:Uuid=tx.query_one("INSERT INTO submissions(user_id,exercise_id,assignment_id,notes,self_checks,rubric,fingerprint) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id",&[&u.id,&b.exercise_id,&b.assignment_id,&notes,&json!(b.self_checks),&exercise,&fingerprint]).await?.get(0);
    tx.execute(
        "UPDATE files SET submission_id=$1 WHERE id=ANY($2)",
        &[&id, &b.file_ids],
    )
    .await?;
    db::audit(
        &tx,
        Some(u.id),
        "submission.created",
        &id.to_string(),
        json!({"track":track,"exercise":b.exercise_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":id,"duplicate":false,"message":"Đã lưu bài. Chờ người chủ trì chấm.","redirect":format!("/bai-nop/{id}")}),
    ))
}
#[derive(Deserialize, Default)]
pub struct Paging {
    pub page: Option<i64>,
}
pub async fn submissions_data(
    s: &AppState,
    v: &Viewer,
    track: &str,
    admin: bool,
    page: i64,
) -> Result<Vec<Value>> {
    access(s, v, track).await?;
    let u = if admin { v.admin()? } else { v.auth()? };
    let c = s.pool.get().await?;
    db::values(&c,"SELECT jsonb_build_object('id',s.id,'title',s.rubric->>'title','exercise_id',s.exercise_id,'user_id',s.user_id,'student_name',u.name,'student_email',u.email,'created_at',s.created_at,'status',s.status,'score',s.result->'score') FROM submissions s JOIN exercises e ON e.id=s.exercise_id JOIN users u ON u.id=s.user_id WHERE e.track=$1 AND ($3 OR s.user_id=$2) ORDER BY s.created_at DESC LIMIT 30 OFFSET $4",&[&track,&u.id,&admin,&((page.clamp(1,10000)-1)*30)]).await
}
async fn submissions(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
    Query(q): Query<Paging>,
) -> Result<Json<Value>> {
    Ok(Json(json!(
        submissions_data(&s, &v, &track, false, q.page.unwrap_or(1)).await?
    )))
}
async fn admin_submissions(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
    Query(q): Query<Paging>,
) -> Result<Json<Value>> {
    Ok(Json(json!(
        submissions_data(&s, &v, &track, true, q.page.unwrap_or(1)).await?
    )))
}
pub async fn detail_data(s: &AppState, v: &Viewer, id: Uuid) -> Result<Value> {
    let u = v.auth()?;
    let c = s.pool.get().await?;
    let mut body=db::value(&c,"SELECT to_jsonb(s)-'fingerprint'||jsonb_build_object('track',e.track,'student_name',u.name,'student_email',u.email) FROM submissions s JOIN exercises e ON e.id=s.exercise_id JOIN users u ON u.id=s.user_id WHERE s.id=$1 AND (s.user_id=$2 OR $3)",&[&id,&u.id,&(u.role=="admin")]).await?;
    if body.is_null() {
        return Err(AppError::missing("Không tìm thấy bài nộp."));
    }
    access(s, v, body["track"].as_str().unwrap()).await?;
    body["files"]=json!(db::values(&c,"SELECT jsonb_build_object('id',id,'name',name,'size',size,'content_type',content_type) FROM files WHERE submission_id=$1 ORDER BY created_at",&[&id]).await?);
    body["reviews"]=json!(db::values(&c,"SELECT jsonb_build_object('result',r.result,'reviewer',u.name,'created_at',r.created_at) FROM reviews r JOIN users u ON u.id=r.reviewer_id WHERE submission_id=$1 ORDER BY r.id DESC",&[&id]).await?);
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
#[serde(deny_unknown_fields)]
struct Criterion {
    id: String,
    status: String,
    evidence: String,
    #[serde(default)]
    action: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    revision: i32,
    summary: String,
    criteria: Vec<Criterion>,
}
async fn review(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
    Json(b): Json<Review>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    let summary = security::text(&b.summary, 5, 2000, "Nhận xét chung")?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let row = tx
        .query_opt(
            "SELECT revision,rubric FROM submissions WHERE id=$1 FOR UPDATE",
            &[&id],
        )
        .await?
        .ok_or_else(|| AppError::missing("Không tìm thấy bài nộp."))?;
    if row.get::<_, i32>("revision") != b.revision {
        return Err(AppError::conflict(
            "Bài đã có nhận xét mới. Tải lại trước khi chấm.",
        ));
    }
    let rubric: Value = row.get("rubric");
    let criteria = rubric["criteria"].as_array().unwrap();
    if b.criteria.len() != criteria.len()
        || b.criteria
            .iter()
            .map(|c| &c.id)
            .collect::<std::collections::HashSet<_>>()
            .len()
            != criteria.len()
    {
        return Err(AppError::bad("Cần chấm đủ các tiêu chí, không trùng."));
    }
    let mut score = 0;
    let mut results = vec![];
    let mut clear = true;
    for criterion in criteria {
        let r = b
            .criteria
            .iter()
            .find(|r| criterion["id"] == r.id)
            .ok_or_else(|| AppError::bad("Tiêu chí không thuộc bài nộp."))?;
        if !["met", "not_met", "unclear"].contains(&r.status.as_str()) {
            return Err(AppError::bad("Kết quả tiêu chí không hợp lệ."));
        }
        let evidence = security::text(&r.evidence, 3, 1500, "Bằng chứng")?;
        let action = security::text(&r.action, 0, 1500, "Hướng sửa")?;
        if r.status == "met" {
            score += criterion["points"].as_i64().unwrap_or(0);
        }
        if r.status == "unclear" {
            clear = false;
        }
        results.push(json!({"id":r.id,"text":criterion["text"],"points":criterion["points"],"status":r.status,"evidence":evidence,"action":action}));
    }
    let status = if score >= rubric["passScore"].as_i64().unwrap_or(75) && clear {
        "passed"
    } else {
        "needs_revision"
    };
    let result = json!({"score":score,"summary":summary,"criteria":results,"status":status});
    tx.execute(
        "INSERT INTO reviews(submission_id,reviewer_id,result) VALUES($1,$2,$3)",
        &[&id, &u.id, &result],
    )
    .await?;
    tx.execute(
        "UPDATE submissions SET status=$2,result=$3,revision=revision+1 WHERE id=$1",
        &[&id, &status, &result],
    )
    .await?;
    db::audit(
        &tx,
        Some(u.id),
        "submission.reviewed",
        &id.to_string(),
        json!({"score":score,"revision":b.revision+1}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã lưu điểm và nhận xét.","result":result,"reload":true}),
    ))
}
