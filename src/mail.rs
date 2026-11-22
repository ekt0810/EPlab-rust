use crate::{
    AppState, db,
    error::{AppError, Result},
    security,
};
use serde_json::json;
use uuid::Uuid;

pub async fn send_token(s: &AppState, id: Uuid, email: &str, purpose: &str) -> Result<()> {
    let token = security::token();
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    tx.query_one("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&id])
        .await?;
    tx.execute(
        "DELETE FROM tokens WHERE user_id=$1 AND purpose=$2",
        &[&id, &purpose],
    )
    .await?;
    tx.execute("INSERT INTO tokens(hash,user_id,purpose,expires_at) VALUES($1,$2,$3,now()+interval '30 minutes')",&[&security::hash(&token),&id,&purpose]).await?;
    tx.commit().await?;
    let (route, subject) = match purpose {
        "reset" => ("/khoi-phuc", "Khôi phục mật khẩu EP Lab"),
        "invite" => ("/nhan-tai-khoan", "Lời mời tham gia EP Lab"),
        _ => ("/xac-minh", "Xác minh email EP Lab"),
    };
    let text = format!(
        "{subject}\n\nMở liên kết để tiếp tục: {}{route}#token={token}\n\nLiên kết dùng một lần, hết hạn sau 30 phút. Nếu bạn không yêu cầu, hãy bỏ qua thư này.",
        s.config.origin
    );
    if s.config.mail_mode == "local" {
        let dir = s.config.data.join("mail");
        tokio::fs::create_dir_all(&dir).await?;
        tokio::fs::write(
            dir.join(format!("{}.json", Uuid::new_v4())),
            serde_json::to_vec_pretty(&json!({"to":email,"subject":subject,"text":text})).unwrap(),
        )
        .await?;
    } else {
        use lettre::{
            AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
            transport::smtp::authentication::Credentials,
        };
        let env =
            |name: &str| std::env::var(name).map_err(|_| AppError::bad("Chưa cấu hình đủ SMTP."));
        let host = env("SMTP_HOST")?;
        let port = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(587);
        let builder = if port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
        }
        .map_err(|_| AppError::bad("Cấu hình máy chủ SMTP không hợp lệ."))?;
        let mail = Message::builder()
            .from(
                env("SMTP_FROM")?
                    .parse()
                    .map_err(|_| AppError::bad("SMTP_FROM không hợp lệ."))?,
            )
            .to(email
                .parse()
                .map_err(|_| AppError::bad("Email không hợp lệ."))?)
            .subject(subject)
            .body(text)
            .map_err(|_| AppError::bad("Không thể soạn thư."))?;
        builder
            .port(port)
            .credentials(Credentials::new(env("SMTP_USER")?, env("SMTP_PASSWORD")?))
            .build()
            .send(mail)
            .await
            .map_err(|_| {
                AppError(
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "Chưa gửi được email. Hãy thử lại hoặc liên hệ người chủ trì.".into(),
                )
            })?;
    }
    db::audit(
        &c,
        None,
        &format!("user.{purpose}_sent"),
        &id.to_string(),
        json!({}),
    )
    .await?;
    Ok(())
}
