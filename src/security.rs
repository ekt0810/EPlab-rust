use crate::{
    AppState,
    error::{AppError, Result},
};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use subtle::ConstantTimeEq;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Actor {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub phone: String,
    pub role: String,
    pub verified: bool,
    pub blocked: bool,
    pub active: bool,
    #[serde(skip)]
    pub session_id: String,
    #[serde(skip)]
    pub csrf: String,
}
#[derive(Clone, Default)]
pub struct Viewer(pub Option<Actor>);
impl Viewer {
    pub fn auth(&self) -> Result<&Actor> {
        self.0
            .as_ref()
            .filter(|u| u.verified && !u.blocked)
            .ok_or_else(AppError::login)
    }
    pub fn admin(&self) -> Result<&Actor> {
        let u = self.auth()?;
        if u.role != "admin" {
            return Err(AppError::forbidden(
                "Chỉ người chủ trì được thực hiện thao tác này.",
            ));
        }
        Ok(u)
    }
    pub fn learn(&self) -> Result<&Actor> {
        let u = self.auth()?;
        if !u.active && u.role != "admin" {
            return Err(AppError::forbidden(
                "Tham gia Lab và được duyệt để học nội dung này.",
            ));
        }
        Ok(u)
    }
}
pub fn token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
pub fn hash(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}
pub fn equal(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}
pub fn valid_token(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn text(s: &str, min: usize, max: usize, label: &str) -> Result<String> {
    let value = s.trim();
    if value.chars().count() < min || value.chars().count() > max || value.contains('\0') {
        return Err(AppError::bad(format!(
            "{label}: cần từ {min} đến {max} ký tự."
        )));
    }
    Ok(value.to_string())
}
pub fn email(s: &str) -> Result<String> {
    let s = text(s, 3, 254, "Email")?.to_lowercase();
    if s.parse::<lettre::Address>().is_err() {
        return Err(AppError::bad("Email không hợp lệ."));
    }
    Ok(s)
}
pub fn phone(s: &str) -> Result<String> {
    let s: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '.')
        .collect();
    let s = if let Some(p) = s.strip_prefix("+84") {
        format!("0{p}")
    } else {
        s
    };
    if s.len() != 10 || !s.starts_with('0') || !s.bytes().all(|c| c.is_ascii_digit()) {
        return Err(AppError::bad(
            "Số điện thoại cần 10 chữ số, bắt đầu bằng 0.",
        ));
    }
    Ok(s)
}
pub async fn guard(State(s): State<AppState>, mut req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    if path.starts_with("/static/") {
        return secure_headers(next.run(req).await);
    }
    let cookie = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let sid = cookie
        .split(';')
        .find_map(|part| part.trim().strip_prefix("ep_session="))
        .filter(|v| valid_token(v));
    let mut viewer = Viewer::default();
    if let Some(sid) = sid {
        let result:Result<Option<Actor>>=async{
            let c=s.pool.get().await?;
            let row=crate::db::row(&c,"SELECT u.id,u.email,u.name,u.phone,u.role,u.verified,u.blocked,(u.role='admin' OR EXISTS(SELECT 1 FROM lab_memberships m WHERE m.user_id=u.id AND m.status='approved')) AS active,s.id AS session_id,s.csrf FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.id=$1 AND s.expires_at>now() AND NOT u.blocked",&[&hash(sid)]).await?;
            Ok(row.map(|r|Actor{id:r.get("id"),email:r.get("email"),name:r.get("name"),phone:r.get("phone"),role:r.get("role"),verified:r.get("verified"),blocked:r.get("blocked"),active:r.get("active"),session_id:r.get("session_id"),csrf:r.get("csrf")}))
        }.await;
        match result {
            Ok(actor) => viewer.0 = actor,
            Err(e) => return secure_headers(e.into_response()),
        }
    }
    let mutation = !matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    );
    if mutation {
        let origin = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if origin != s.config.origin {
            return secure_headers(
                AppError::forbidden("Nguồn yêu cầu không hợp lệ.").into_response(),
            );
        }
        if let Some(u) = &viewer.0 {
            let csrf = req
                .headers()
                .get("x-csrf-token")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !valid_token(csrf) || !equal(csrf, &u.csrf) {
                return secure_headers(
                    AppError::forbidden("Phiên xác thực đã thay đổi. Tải lại trang.")
                        .into_response(),
                );
            }
        }
    }
    let ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|v| v.0.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".into());
    let key = viewer
        .0
        .as_ref()
        .map(|u| format!("user:{}", u.id))
        .unwrap_or_else(|| format!("ip:{ip}"));
    if mutation {
        let quota = match path.as_str() {
            "/api/auth/register" => Some(8),
            "/api/auth/login" => Some(20),
            "/api/auth/forgot" | "/api/auth/resend" => Some(5),
            "/api/auth/reset" | "/api/auth/accept-invitation" => Some(10),
            _ => None,
        };
        if let Some(maximum) = quota
            && let Err(error) =
                crate::auth::rate(&s, &format!("auth-ip:{ip}:{path}"), maximum).await
        {
            return secure_headers(error.into_response());
        }
    }
    let (allowed, remaining, reset) = {
        let mut limits = s.limits.lock().unwrap_or_else(|p| p.into_inner());
        if limits.len() > 10_000 {
            limits.retain(|_, (_, t)| t.elapsed().as_secs() < 60);
        }
        let item = limits.entry(key).or_insert((0, std::time::Instant::now()));
        if item.1.elapsed().as_secs() >= 60 {
            *item = (0, std::time::Instant::now());
        }
        item.0 = item.0.saturating_add(1);
        (
            item.0 <= 180,
            180u32.saturating_sub(item.0),
            60 - item.1.elapsed().as_secs().min(59),
        )
    };
    if !allowed {
        let mut r = AppError(
            StatusCode::TOO_MANY_REQUESTS,
            "Thao tác quá nhanh. Vui lòng thử lại sau một phút.".into(),
        )
        .into_response();
        r.headers_mut()
            .insert("retry-after", reset.to_string().parse().unwrap());
        return secure_headers(r);
    }
    req.extensions_mut().insert(viewer);
    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "x-ratelimit-remaining",
        remaining.to_string().parse().unwrap(),
    );
    response
        .headers_mut()
        .insert("x-ratelimit-reset", reset.to_string().parse().unwrap());
    secure_headers(response)
}
fn secure_headers(mut response: Response) -> Response {
    let h = response.headers_mut();
    h.insert("x-content-type-options", "nosniff".parse().unwrap());
    h.insert("x-frame-options", "DENY".parse().unwrap());
    h.insert(
        "permissions-policy",
        "camera=(), microphone=(), geolocation=()".parse().unwrap(),
    );
    h.insert("vary", "Cookie".parse().unwrap());
    h.insert(
        "referrer-policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    h.insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; frame-src https://www.youtube-nocookie.com; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'; object-src 'none'".parse().unwrap());
    h.entry(header::CACHE_CONTROL)
        .or_insert("private, no-store".parse().unwrap());
    response
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_contact_and_text() {
        assert!(email("a@b.test").is_ok());
        assert!(email("bad").is_err());
        assert_eq!(phone("+84 900 000 001").unwrap(), "0900000001");
        assert!(phone("abc").is_err());
        assert!(text("  ", 1, 20, "x").is_err());
    }
    #[test]
    fn token_is_random_and_hashed() {
        let a = token();
        assert!(valid_token(&a));
        assert_ne!(a, token());
        assert_ne!(a, hash(&a));
        assert!(equal(&a, &a));
        assert!(!equal(&a, &token()));
    }
}
