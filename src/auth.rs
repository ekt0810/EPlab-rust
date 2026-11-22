use crate::{
    AppState, db,
    error::{AppError, Result},
    mail,
    security::{self, Viewer},
};
use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version,
    password_hash::{PasswordHash, SaltString, rand_core::OsRng},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/me", get(me))
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/verify", post(verify))
        .route("/api/auth/resend", post(resend))
        .route("/api/auth/forgot", post(forgot))
        .route("/api/auth/reset", post(reset))
        .route("/api/auth/accept-invitation", post(accept_invite))
        .route("/api/auth/profile", patch(profile))
        .route("/api/auth/password", post(change_password))
        .route("/api/auth/sessions", get(sessions))
        .route("/api/auth/sessions/{id}", delete(revoke_session))
}
pub async fn hash_password(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        let a = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(19456, 2, 1, None).unwrap(),
        );
        a.hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|h| h.to_string())
    })
    .await
    .map_err(|_| AppError::bad("Không thể xử lý mật khẩu."))?
    .map_err(|_| AppError::bad("Không thể xử lý mật khẩu."))
}
async fn check_password(password: String, encoded: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&encoded).is_ok_and(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false)
}
fn valid_password(password: &str) -> Result<()> {
    if password.len() < 12 || password.len() > 128 {
        return Err(AppError::bad("Mật khẩu cần 12–128 ký tự."));
    }
    Ok(())
}
pub async fn rate(s: &AppState, key: &str, maximum: i32) -> Result<()> {
    let c = s.pool.get().await?;
    let attempts:i32=c.query_one("INSERT INTO rate_limits(key) VALUES($1) ON CONFLICT(key) DO UPDATE SET attempts=CASE WHEN rate_limits.window_start<now()-interval '15 minutes' THEN 1 ELSE LEAST(rate_limits.attempts+1,1000) END,window_start=CASE WHEN rate_limits.window_start<now()-interval '15 minutes' THEN now() ELSE rate_limits.window_start END RETURNING attempts",&[&security::hash(key)]).await?.get(0);
    if attempts > maximum {
        return Err(AppError(
            StatusCode::TOO_MANY_REQUESTS,
            "Vui lòng thử lại sau 15 phút.".into(),
        ));
    }
    Ok(())
}
async fn me(Extension(v): Extension<Viewer>) -> Json<Value> {
    Json(json!({"user":v.0,"csrf":v.0.as_ref().map(|u|&u.csrf)}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Register {
    name: String,
    email: String,
    phone: String,
    password: String,
}
async fn register(State(s): State<AppState>, Json(b): Json<Register>) -> Result<Json<Value>> {
    let name = security::text(&b.name, 2, 80, "Họ tên")?;
    let email = security::email(&b.email)?;
    let phone = security::phone(&b.phone)?;
    valid_password(&b.password)?;
    rate(&s, &format!("email:{email}"), 3).await?;
    let _permit = s.hashes.acquire().await.unwrap();
    let hashed = hash_password(b.password).await?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let row=tx.query_opt("INSERT INTO users(name,email,phone,password_hash) VALUES($1,$2,$3,$4) ON CONFLICT(email) DO NOTHING RETURNING id",&[&name,&email,&phone,&hashed]).await?;
    if let Some(r) = row {
        let id: Uuid = r.get(0);
        db::audit(&tx, Some(id), "user.registered", &id.to_string(), json!({})).await?;
        tx.commit().await?;
        drop(c);
        mail::send_token(&s, id, &email, "verify").await?;
    }
    Ok(Json(
        json!({"message":"Nếu email đủ điều kiện, thư xác minh đã được gửi. Kiểm tra hộp thư để tiếp tục."}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    email: String,
    password: String,
}
async fn login(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<Login>,
) -> Result<Response> {
    let email = security::email(&b.email)?;
    if b.password.len() > 128 {
        return Err(AppError::bad("Mật khẩu quá dài."));
    }
    rate(&s, &format!("login:{email}"), 10).await?;
    let mut c = s.pool.get().await?;
    let row = c
        .query_opt(
            "SELECT id,password_hash,verified,blocked,role FROM users WHERE email=$1",
            &[&email],
        )
        .await?;
    let encoded = row
        .as_ref()
        .map(|r| r.get::<_, String>("password_hash"))
        .unwrap_or_else(|| s.dummy_hash.to_string());
    let _permit = s.hashes.acquire().await.unwrap();
    if !check_password(b.password, encoded.clone()).await || row.is_none() {
        return Err(AppError(
            StatusCode::UNAUTHORIZED,
            "Email hoặc mật khẩu không đúng.".into(),
        ));
    }
    let row = row.unwrap();
    if row.get::<_, bool>("blocked") {
        return Err(AppError::forbidden("Tài khoản đang bị khóa."));
    }
    if !row.get::<_, bool>("verified") {
        return Err(AppError::forbidden("Xác minh email trước khi đăng nhập."));
    }
    let id: Uuid = row.get("id");
    let sid = security::token();
    let csrf = security::token();
    let tx = c.transaction().await?;
    let current = tx
        .query_one(
            "SELECT password_hash,blocked,verified,setup_pending FROM users WHERE id=$1 FOR UPDATE",
            &[&id],
        )
        .await?;
    if current.get::<_, String>("password_hash") != encoded
        || current.get::<_, bool>("blocked")
        || !current.get::<_, bool>("verified")
        || current.get::<_, bool>("setup_pending")
    {
        return Err(AppError::conflict("Tài khoản đã thay đổi. Đăng nhập lại."));
    }
    tx.execute("INSERT INTO sessions(id,user_id,csrf,user_agent,expires_at) VALUES($1,$2,$3,$4,now()+interval '7 days')",&[&security::hash(&sid),&id,&csrf,&headers.get(header::USER_AGENT).and_then(|h|h.to_str().ok()).unwrap_or("").chars().take(300).collect::<String>()]).await?;
    tx.execute("DELETE FROM sessions WHERE user_id=$1 AND (expires_at<=now() OR id IN(SELECT id FROM sessions WHERE user_id=$1 ORDER BY created_at DESC,id DESC OFFSET 10))",&[&id]).await?;
    tx.execute("UPDATE users SET last_login_at=now() WHERE id=$1", &[&id])
        .await?;
    tx.execute(
        "DELETE FROM rate_limits WHERE key=$1",
        &[&security::hash(&format!("login:{email}"))],
    )
    .await?;
    db::audit(&tx, Some(id), "login", &id.to_string(), json!({})).await?;
    tx.commit().await?;
    let mut response=Json(json!({"ok":true,"csrf":csrf,"redirect":if row.get::<_,String>("role")=="admin"{"/quan-tri"}else{"/"}})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        session_cookie(&s, &sid, 604800).parse().unwrap(),
    );
    Ok(response)
}
fn session_cookie(s: &AppState, sid: &str, age: u32) -> String {
    format!(
        "ep_session={sid}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}{}",
        if s.config.secure { "; Secure" } else { "" }
    )
}
async fn logout(State(s): State<AppState>, Extension(v): Extension<Viewer>) -> Result<Response> {
    if let Some(u) = v.0 {
        let c = s.pool.get().await?;
        c.execute("DELETE FROM sessions WHERE id=$1", &[&u.session_id])
            .await?;
        db::audit(&c, Some(u.id), "logout", &u.id.to_string(), json!({})).await?;
    }
    let mut response = Json(json!({"ok":true,"redirect":"/"})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        session_cookie(&s, "", 0).parse().unwrap(),
    );
    Ok(response)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Token {
    token: String,
}
async fn verify(State(s): State<AppState>, Json(b): Json<Token>) -> Result<Json<Value>> {
    if !security::valid_token(&b.token) {
        return Err(AppError::bad("Liên kết không hợp lệ."));
    }
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let row=tx.query_opt("DELETE FROM tokens WHERE hash=$1 AND purpose='verify' AND expires_at>now() RETURNING user_id",&[&security::hash(&b.token)]).await?.ok_or_else(||AppError::bad("Liên kết đã dùng hoặc hết hạn."))?;
    let id: Uuid = row.get(0);
    if tx
        .execute(
            "UPDATE users SET verified=true WHERE id=$1 AND NOT blocked AND NOT setup_pending",
            &[&id],
        )
        .await?
        != 1
    {
        return Err(AppError::forbidden("Tài khoản chưa đủ điều kiện xác minh."));
    }
    db::audit(&tx, Some(id), "user.verified", &id.to_string(), json!({})).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Email đã xác minh. Bạn có thể đăng nhập.","redirect":"/dang-nhap"}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Email {
    email: String,
}
async fn resend(State(s): State<AppState>, Json(b): Json<Email>) -> Result<Json<Value>> {
    send_recovery(&s, b.email, "verify").await
}
async fn forgot(State(s): State<AppState>, Json(b): Json<Email>) -> Result<Json<Value>> {
    send_recovery(&s, b.email, "reset").await
}
async fn send_recovery(s: &AppState, address: String, purpose: &str) -> Result<Json<Value>> {
    let email = security::email(&address)?;
    rate(s, &format!("email:{email}"), 3).await?;
    let c = s.pool.get().await?;
    let row = c
        .query_opt(
            "SELECT id,verified,setup_pending FROM users WHERE email=$1 AND NOT blocked",
            &[&email],
        )
        .await?;
    drop(c);
    if let Some(r) = row
        && !r.get::<_, bool>("setup_pending")
        && (purpose == "reset" || !r.get::<_, bool>("verified"))
    {
        mail::send_token(s, r.get(0), &email, purpose).await?;
    }
    Ok(Json(
        json!({"message":"Nếu tài khoản đủ điều kiện, liên kết đã được gửi tới email."}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Reset {
    token: String,
    password: String,
    #[serde(default)]
    accepted_terms: bool,
}
async fn reset(State(s): State<AppState>, Json(b): Json<Reset>) -> Result<Json<Value>> {
    consume_password_token(&s, b, "reset").await
}
async fn accept_invite(State(s): State<AppState>, Json(b): Json<Reset>) -> Result<Json<Value>> {
    if !b.accepted_terms {
        return Err(AppError::bad("Đồng ý điều khoản trước khi nhận tài khoản."));
    }
    consume_password_token(&s, b, "invite").await
}
async fn consume_password_token(s: &AppState, b: Reset, purpose: &str) -> Result<Json<Value>> {
    valid_password(&b.password)?;
    if !security::valid_token(&b.token) {
        return Err(AppError::bad("Liên kết không hợp lệ."));
    }
    let _permit = s.hashes.acquire().await.unwrap();
    let hashed = hash_password(b.password).await?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let r=tx.query_opt("SELECT t.user_id FROM tokens t JOIN users u ON u.id=t.user_id WHERE t.hash=$1 AND t.purpose=$2 AND t.expires_at>now() AND NOT u.blocked FOR UPDATE OF t,u",&[&security::hash(&b.token),&purpose]).await?.ok_or_else(||AppError::bad("Liên kết đã dùng hoặc hết hạn."))?;
    let id: Uuid = r.get(0);
    tx.execute(
        "UPDATE users SET password_hash=$2,verified=true,setup_pending=false WHERE id=$1",
        &[&id, &hashed],
    )
    .await?;
    tx.execute("DELETE FROM tokens WHERE user_id=$1", &[&id])
        .await?;
    tx.execute("DELETE FROM sessions WHERE user_id=$1", &[&id])
        .await?;
    db::audit(
        &tx,
        Some(id),
        if purpose == "invite" {
            "user.invitation_accepted"
        } else {
            "user.password_reset"
        },
        &id.to_string(),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã lưu mật khẩu. Hãy đăng nhập.","redirect":"/dang-nhap"}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    name: String,
    phone: String,
}
async fn profile(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Json(b): Json<Profile>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    let name = security::text(&b.name, 2, 80, "Họ tên")?;
    let phone = security::phone(&b.phone)?;
    let c = s.pool.get().await?;
    c.execute(
        "UPDATE users SET name=$2,phone=$3 WHERE id=$1",
        &[&u.id, &name, &phone],
    )
    .await?;
    db::audit(
        &c,
        Some(u.id),
        "user.profile_updated",
        &u.id.to_string(),
        json!({}),
    )
    .await?;
    Ok(Json(json!({"message":"Đã lưu hồ sơ."})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Password {
    current: String,
    password: String,
}
async fn change_password(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Json(b): Json<Password>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    valid_password(&b.password)?;
    rate(&s, &format!("password:{}", u.id), 10).await?;
    let mut c = s.pool.get().await?;
    let encoded: String = c
        .query_one("SELECT password_hash FROM users WHERE id=$1", &[&u.id])
        .await?
        .get(0);
    let _permit = s.hashes.acquire().await.unwrap();
    if !check_password(b.current, encoded.clone()).await {
        return Err(AppError::bad("Mật khẩu hiện tại không đúng."));
    }
    let hashed = hash_password(b.password).await?;
    let tx = c.transaction().await?;
    let changed = tx
        .execute(
            "UPDATE users SET password_hash=$2 WHERE id=$1 AND password_hash=$3",
            &[&u.id, &hashed, &encoded],
        )
        .await?;
    if changed != 1 {
        return Err(AppError::conflict("Mật khẩu vừa thay đổi. Đăng nhập lại."));
    }
    tx.execute("DELETE FROM sessions WHERE user_id=$1", &[&u.id])
        .await?;
    tx.execute("DELETE FROM tokens WHERE user_id=$1", &[&u.id])
        .await?;
    db::audit(
        &tx,
        Some(u.id),
        "user.password_changed",
        &u.id.to_string(),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã đổi mật khẩu và đăng xuất các phiên.","redirect":"/dang-nhap"}),
    ))
}
async fn sessions(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    let c = s.pool.get().await?;
    Ok(Json(json!(db::values(&c,"SELECT jsonb_build_object('id',id,'user_agent',user_agent,'created_at',created_at,'expires_at',expires_at,'current',id=$2) FROM sessions WHERE user_id=$1 AND expires_at>now() ORDER BY created_at DESC",&[&u.id,&u.session_id]).await?)))
}
async fn revoke_session(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    let c = s.pool.get().await?;
    let n = c
        .execute(
            "DELETE FROM sessions WHERE id=$1 AND user_id=$2",
            &[&id, &u.id],
        )
        .await?;
    if n == 0 {
        return Err(AppError::missing("Không tìm thấy phiên."));
    }
    db::audit(
        &c,
        Some(u.id),
        "session.revoked",
        &u.id.to_string(),
        json!({}),
    )
    .await?;
    Ok(Json(json!({"message":"Đã đăng xuất phiên."})))
}
pub async fn create_admin(s: &AppState) -> anyhow::Result<()> {
    let email =
        security::email(&std::env::var("ADMIN_EMAIL")?).map_err(|e| anyhow::anyhow!(e.1))?;
    let name = std::env::var("ADMIN_NAME").unwrap_or_else(|_| "Người chủ trì".into());
    let password = std::env::var("ADMIN_PASSWORD")?;
    valid_password(&password).map_err(|e| anyhow::anyhow!(e.1))?;
    let encoded = hash_password(password)
        .await
        .map_err(|e| anyhow::anyhow!(e.1))?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    tx.execute("SELECT pg_advisory_xact_lock(8493101)", &[])
        .await?;
    let existing = tx
        .query_opt("SELECT id,role FROM users WHERE email=$1", &[&email])
        .await?;
    if existing.is_some() {
        anyhow::bail!("Account already exists; this command never changes an existing account");
    }
    let slot:i16=tx.query_opt("SELECT n::smallint FROM generate_series(1,2) n WHERE NOT EXISTS(SELECT 1 FROM users WHERE admin_slot=n) ORDER BY n LIMIT 1",&[]).await?.ok_or_else(||anyhow::anyhow!("Both administrator positions are occupied"))?.get(0);
    tx.execute("INSERT INTO users(email,name,password_hash,verified,role,admin_slot) VALUES($1,$2,$3,true,'admin',$4)",&[&email,&name,&encoded,&slot]).await?;
    tx.commit().await?;
    println!("Created administrator in slot {slot}; password is not printed.");
    Ok(())
}
