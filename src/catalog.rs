use crate::{
    AppState, db,
    error::{AppError, Result},
    security::{self, Viewer},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub struct Content {
    pub courses: HashMap<String, Value>,
    pub sources: Value,
    pub exercises: Vec<Value>,
}
impl Content {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            courses: serde_json::from_str(include_str!("../content/courses.json"))?,
            sources: serde_json::from_str(include_str!("../content/sources.json"))?,
            exercises: serde_json::from_str(include_str!("../content/exercises.json"))?,
        })
    }
}
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/classroom", get(index))
        .route("/api/classroom/progress", get(all_progress))
        .route("/api/classroom/{track}", get(course))
        .route("/api/classroom/{track}/sources", get(sources))
        .route("/api/classroom/{track}/lessons/{id}", get(lesson))
        .route(
            "/api/classroom/{track}/lessons/{id}/progress",
            put(progress),
        )
        .route("/api/classroom/{track}/lessons/{id}/quiz", post(quiz))
        .route("/api/classroom/{track}/lessons/{id}/questions", post(ask))
}
pub async fn seed(s: &AppState) -> anyhow::Result<()> {
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    tx.batch_execute("SELECT pg_advisory_xact_lock(8493102)")
        .await?;
    if tx
        .query_opt("SELECT version FROM content_seeds WHERE version=1", &[])
        .await?
        .is_some()
    {
        tx.commit().await?;
        return Ok(());
    }
    for (position, track) in ["embedded", "kicad", "solidworks"].iter().enumerate() {
        let body = &s.content.courses[*track];
        tx.execute("INSERT INTO courses(category,title,description,position) VALUES($1,$2,$3,$4) ON CONFLICT(category) DO NOTHING",&[track,&body["title"].as_str().unwrap(),&body["description"].as_str().unwrap(),&(position as i32)]).await?;
        for (i, l) in body["lessons"].as_array().unwrap().iter().enumerate() {
            tx.execute("INSERT INTO lessons(track,id,body,position) VALUES($1,$2,$3,$4) ON CONFLICT(track,id) DO NOTHING",&[track,&l["id"].as_str().unwrap(),l,&(i as i32)]).await?;
        }
    }
    for e in &s.content.exercises {
        tx.execute(
            "INSERT INTO exercises(id,track,body) VALUES($1,$2,$3) ON CONFLICT(id) DO NOTHING",
            &[&e["id"].as_str().unwrap(), &e["track"].as_str().unwrap(), e],
        )
        .await?;
    }
    tx.execute("INSERT INTO content_seeds(version) VALUES(1)", &[])
        .await?;
    tx.commit().await?;
    Ok(())
}
pub fn track_name(track: &str) -> &'static str {
    match track {
        "embedded" => "Lập trình nhúng C",
        "kicad" => "Thiết kế mạch KiCad",
        "solidworks" => "Thiết kế cơ khí SOLIDWORKS",
        _ => "Môn học",
    }
}
pub fn valid_track(s: &str) -> Result<&str> {
    if ["embedded", "kicad", "solidworks"].contains(&s) {
        Ok(s)
    } else {
        Err(AppError::missing("Không tìm thấy môn học."))
    }
}
pub async fn readable(s: &AppState, v: &Viewer, track: &str, id: &str) -> Result<Value> {
    valid_track(track)?;
    let c = s.pool.get().await?;
    let r=db::row(&c,"SELECT l.body,l.status,c.status AS course_status FROM lessons l JOIN courses c ON c.category=l.track WHERE l.track=$1 AND l.id=$2",&[&track,&id]).await?.ok_or_else(||AppError::missing("Không tìm thấy bài học."))?;
    let body: Value = r.get("body");
    let admin =
        v.0.as_ref()
            .is_some_and(|u| u.role == "admin" && u.verified);
    if !admin
        && (r.get::<_, String>("status") != "published"
            || r.get::<_, String>("course_status") != "published")
    {
        return Err(AppError::missing("Bài học chưa được mở."));
    }
    if !admin && !body["preview"].as_bool().unwrap_or(false) {
        v.learn()?;
    }
    Ok(body)
}
pub fn public_lesson(mut lesson: Value) -> Value {
    if let Some(quiz) = lesson.get_mut("quiz").and_then(Value::as_object_mut) {
        quiz.remove("answer");
        quiz.remove("explanation");
    }
    if let Some(questions) = lesson.get_mut("questions") {
        *questions = Value::Array(
            questions
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|q| q[0].clone())
                .collect(),
        );
    }
    lesson
}
pub async fn course_index(s: &AppState, v: &Viewer) -> Result<Vec<Value>> {
    let c = s.pool.get().await?;
    let admin = v.0.as_ref().is_some_and(|u| u.role == "admin");
    let rows=db::rows(&c,"SELECT category,title,description,status,(SELECT count(*) FROM lessons l WHERE l.track=c.category AND (l.status='published' OR $1)) AS lesson_count,(SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'title',body->>'title','minutes',body->'minutes') ORDER BY position),'[]') FROM lessons l WHERE l.track=c.category AND body->>'preview'='true' AND (l.status='published' OR $1)) AS previews FROM courses c WHERE status='published' OR $1 ORDER BY position",&[&admin]).await?;
    Ok(rows.iter().map(|r|{
        let track:String=r.get("category");let course=&s.content.courses[&track];
        json!({"track":track,"title":r.get::<_,String>("title"),"description":r.get::<_,String>("description"),"version":course["version"],"lessonCount":r.get::<_,i64>("lesson_count"),"moduleCount":course["modules"].as_array().unwrap().len(),"previews":r.get::<_,Value>("previews")})
    }).collect())
}
async fn index(State(s): State<AppState>, Extension(v): Extension<Viewer>) -> Result<Json<Value>> {
    Ok(Json(json!(course_index(&s, &v).await?)))
}
pub async fn course_data(s: &AppState, v: &Viewer, track: &str) -> Result<Value> {
    valid_track(track)?;
    let c = s.pool.get().await?;
    let admin = v.0.as_ref().is_some_and(|u| u.role == "admin");
    let row=db::row(&c,"SELECT title,description,status FROM courses WHERE category=$1 AND (status='published' OR $2)",&[&track,&admin]).await?.ok_or_else(||AppError::missing("Môn học chưa được mở."))?;
    let lessons=db::values(&c,"SELECT jsonb_build_object('id',id,'module',body->'module','title',body->'title','summary',body->'summary','minutes',body->'minutes','level',body->'level','preview',body->'preview','objectives',body->'objectives') FROM lessons WHERE track=$1 AND (status='published' OR $2) ORDER BY position,id",&[&track,&admin]).await?;
    let progress = if let Ok(u) = v.auth() {
        db::values(&c,"SELECT jsonb_build_object('lesson_id',lesson_id,'scene',scene,'quiz_passed',quiz_passed,'completed',completed,'updated_at',updated_at) FROM classroom_progress WHERE user_id=$1 AND track=$2",&[&u.id,&track]).await?
    } else {
        vec![]
    };
    let mut body = Value::Object(
        s.content.courses[track]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.as_str() != "lessons")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    );
    body["title"] = json!(row.get::<_, String>("title"));
    body["description"] = json!(row.get::<_, String>("description"));
    body["lessons"]=json!(lessons.iter().map(|l|json!({"id":l["id"],"module":l["module"],"title":l["title"],"summary":l["summary"],"minutes":l["minutes"],"level":l["level"],"preview":l["preview"],"objectives":l["objectives"],"accessible":admin||l["preview"]==true||v.0.as_ref().is_some_and(|u|u.active),"progress":progress.iter().find(|p|p["lesson_id"]==l["id"])})).collect::<Vec<_>>());
    body["handbook"] =
        json!({"url":format!("/api/classroom/{track}/handbook"),"accessible":v.learn().is_ok()});
    Ok(body)
}
async fn course(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Json<Value>> {
    Ok(Json(course_data(&s, &v, &track).await?))
}
pub fn lesson_sources(s: &AppState, track: &str, lesson: &Value) -> Vec<Value> {
    lesson["videos"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|reference| {
            s.content.sources["sources"]
                .as_array()?
                .iter()
                .find(|source| {
                    source["id"] == reference["id"]
                        && source["track"] == track
                        && source["kind"] != "excluded"
                        && source["kind"] != "unavailable"
                })
                .map(|source| {
                    let mut source = source.clone();
                    source["start"] = reference.get("start").cloned().unwrap_or(json!(0));
                    source
                })
        })
        .collect()
}
async fn sources(State(s): State<AppState>, Path(track): Path<String>) -> Result<Json<Value>> {
    valid_track(&track)?;
    Ok(Json(json!(
        s.content.sources["sources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["track"] == track
                && v["kind"] != "excluded"
                && v["kind"] != "unavailable")
            .collect::<Vec<_>>()
    )))
}
pub async fn lesson_data(s: &AppState, v: &Viewer, track: &str, id: &str) -> Result<Value> {
    let l = readable(s, v, track, id).await?;
    let c = s.pool.get().await?;
    let (progress, questions) = if let Ok(u) = v.auth() {
        (db::value(&c,"SELECT to_jsonb(p)-'user_id' FROM classroom_progress p WHERE user_id=$1 AND track=$2 AND lesson_id=$3",&[&u.id,&track,&id]).await?,
        db::values(&c,"SELECT jsonb_build_object('id',q.id,'question',q.question,'answer',q.answer,'created_at',q.created_at) FROM classroom_questions q WHERE q.user_id=$1 AND q.track=$2 AND q.lesson_id=$3 AND NOT EXISTS (SELECT 1 FROM jsonb_array_elements(coalesce(q.answer->'references','[]')) ref LEFT JOIN lessons l ON l.track=$2 AND l.id=ref->>'id' WHERE l.id IS NULL OR (NOT $5 AND (l.status!='published' OR (l.body->>'preview'!='true' AND NOT $4)))) ORDER BY q.created_at DESC LIMIT 30",&[&u.id,&track,&id,&u.active,&(u.role=="admin")]).await?)
    } else {
        (Value::Null, vec![])
    };
    let resources=db::values(&c,"SELECT jsonb_build_object('id',id,'name',name,'size',size) FROM files WHERE lesson_track=$1 AND lesson_id=$2 ORDER BY created_at",&[&track,&id]).await?;
    Ok(
        json!({"lesson":public_lesson(l.clone()),"sources":lesson_sources(s,track,&l),"progress":progress,"questions":questions,"resources":resources}),
    )
}
async fn lesson(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
) -> Result<Json<Value>> {
    Ok(Json(lesson_data(&s, &v, &track, &id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Progress {
    #[serde(default)]
    scene: i32,
    #[serde(default)]
    note: String,
    #[serde(default)]
    checklist: Vec<usize>,
    #[serde(default)]
    completed: bool,
}
async fn progress(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
    Json(b): Json<Progress>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    let l = readable(&s, &v, &track, &id).await?;
    let note = security::text(&b.note, 0, 10000, "Ghi chú")?;
    let steps = l["steps"].as_array().map_or(0, Vec::len);
    let unique: HashSet<_> = b.checklist.iter().collect();
    if b.scene < 0
        || b.scene > 100
        || unique.len() != b.checklist.len()
        || b.checklist.iter().any(|n| *n >= steps)
    {
        return Err(AppError::bad("Các bước thực hành không hợp lệ."));
    }
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    tx.execute("INSERT INTO classroom_progress(user_id,track,lesson_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&u.id,&track,&id]).await?;
    let passed:bool=tx.query_one("SELECT quiz_passed FROM classroom_progress WHERE user_id=$1 AND track=$2 AND lesson_id=$3 FOR UPDATE",&[&u.id,&track,&id]).await?.get(0);
    if b.completed && (!passed || b.checklist.len() != steps) {
        return Err(AppError::bad(
            "Hoàn thành tự kiểm và đủ bước thực hành trước khi hoàn tất bài.",
        ));
    }
    tx.execute("UPDATE classroom_progress SET scene=$4,note=$5,checklist=$6,completed=completed OR $7,updated_at=now() WHERE user_id=$1 AND track=$2 AND lesson_id=$3",&[&u.id,&track,&id,&b.scene,&note,&json!(b.checklist),&b.completed]).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":if b.completed{"Bạn đã hoàn thành bài học."}else{"Đã lưu tiến độ và ghi chú."}}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    answer: usize,
}
async fn quiz(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
    Json(b): Json<Answer>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    let l = readable(&s, &v, &track, &id).await?;
    if b.answer >= l["quiz"]["options"].as_array().unwrap().len() {
        return Err(AppError::bad("Lựa chọn không hợp lệ."));
    }
    let correct = l["quiz"]["answer"].as_u64() == Some(b.answer as u64);
    let c = s.pool.get().await?;
    c.execute("INSERT INTO classroom_progress(user_id,track,lesson_id,quiz_passed) VALUES($1,$2,$3,$4) ON CONFLICT(user_id,track,lesson_id) DO UPDATE SET quiz_passed=classroom_progress.quiz_passed OR $4,updated_at=now()",&[&u.id,&track,&id,&correct]).await?;
    Ok(Json(
        json!({"correct":correct,"explanation":l["quiz"]["explanation"]}),
    ))
}
async fn all_progress(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    let c = s.pool.get().await?;
    Ok(Json(json!(db::values(&c,"SELECT to_jsonb(p)-'user_id'||jsonb_build_object('title',l.body->>'title') FROM classroom_progress p JOIN lessons l ON l.track=p.track AND l.id=p.lesson_id JOIN courses c ON c.category=p.track WHERE p.user_id=$1 AND (c.status='published' AND l.status='published' OR $2) ORDER BY p.updated_at DESC",&[&u.id,&(u.role=="admin")]).await?)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Question {
    question: String,
}
async fn ask(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
    Json(b): Json<Question>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    readable(&s, &v, &track, &id).await?;
    let question = security::text(&b.question, 3, 1000, "Câu hỏi")?;
    let c = s.pool.get().await?;
    let all = db::values(
        &c,
        "SELECT body FROM lessons WHERE track=$1 AND status='published' ORDER BY position",
        &[&track],
    )
    .await?;
    let terms: Vec<_> = question
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.chars().count() > 2)
        .map(String::from)
        .collect();
    let readable: Vec<_> = all
        .iter()
        .filter(|l| u.active || u.role == "admin" || l["preview"] == true)
        .collect();
    let best = readable
        .iter()
        .filter_map(|l| {
            let hay = format!("{} {} {}", l["title"], l["summary"], l["concepts"]).to_lowercase();
            let score = terms
                .iter()
                .filter(|term| hay.contains(term.as_str()))
                .count();
            (score > 0).then_some((score, *l))
        })
        .max_by_key(|(score, _)| *score);
    let answer = if let Some((_, l)) = best {
        json!({"mode":"handbook","text":l["concepts"].as_array().unwrap_or(&vec![]).iter().take(3).map(|v|format!("{}: {}",v[0].as_str().unwrap_or(""),v[1].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n\n"),"references":[{"id":l["id"],"title":l["title"],"url":format!("/lop-hoc/{track}/{}",l["id"].as_str().unwrap())}],"videos":lesson_sources(&s,&track,l)})
    } else {
        json!({"mode":"no-match","text":"Chưa tìm thấy nội dung phù hợp trong giáo trình. Hãy hỏi cụ thể một khái niệm hoặc gửi trao đổi cho người chủ trì.","references":[],"videos":[]})
    };
    c.execute("INSERT INTO classroom_questions(user_id,track,lesson_id,question,answer) VALUES($1,$2,$3,$4,$5)",&[&u.id,&track,&id,&question,&answer]).await?;
    Ok(Json(answer))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_curriculum_and_answers_are_private() {
        let c = Content::load().unwrap();
        assert_eq!(
            c.courses
                .values()
                .map(|v| v["lessons"].as_array().unwrap().len())
                .sum::<usize>(),
            100
        );
        assert_eq!(c.exercises.len(), 36);
        for course in c.courses.values() {
            for l in course["lessons"].as_array().unwrap() {
                let p = public_lesson(l.clone());
                assert!(p["quiz"].get("answer").is_none());
                assert!(p["quiz"].get("explanation").is_none());
            }
        }
    }
}
