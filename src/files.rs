use crate::{
    AppState, catalog, db,
    error::{AppError, Result},
    security::Viewer,
};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;
pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/uploads",
            post(upload).layer(DefaultBodyLimit::max(12 * 1024 * 1024)),
        )
        .route("/api/files/{id}", get(download).delete(remove))
        .route("/api/classroom/{track}/handbook", get(handbook))
        .route("/api/admin/handbooks/{track}", get(host_handbook))
        .route(
            "/api/admin/lessons/{track}/{id}/resources",
            post(attach_resource),
        )
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Resource {
    file_id: Uuid,
}
async fn attach_resource(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path((track, id)): Path<(String, String)>,
    Json(b): Json<Resource>,
) -> Result<Json<Value>> {
    let u = v.admin()?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    if tx
        .query_opt(
            "SELECT id FROM lessons WHERE track=$1 AND id=$2 FOR UPDATE",
            &[&track, &id],
        )
        .await?
        .is_none()
    {
        return Err(AppError::missing("Không tìm thấy bài học."));
    }
    if tx.execute("UPDATE files SET lesson_track=$2,lesson_id=$3 WHERE id=$1 AND owner_id=$4 AND submission_id IS NULL AND lesson_id IS NULL",&[&b.file_id,&track,&id,&u.id]).await?!=1{return Err(AppError::bad("Tệp không thuộc tài khoản hoặc đã được sử dụng."));}
    db::audit(
        &tx,
        Some(u.id),
        "lesson.resource_added",
        &format!("{track}/{id}"),
        json!({"fileId":b.file_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"message":"Đã gắn tệp học liệu.","reload":true}),
    ))
}
fn validate_file(name: &str, bytes: &[u8]) -> Result<(String, String, Vec<u8>)> {
    if name.len() > 180
        || name.contains(['/', '\\', '\r', '\n', '\0'])
        || name.starts_with('.')
        || bytes.is_empty()
        || bytes.len() > 10 * 1024 * 1024
    {
        return Err(AppError::bad(
            "Tệp phải có tên hợp lệ và dung lượng 1 byte–10 MB.",
        ));
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" => {
            let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|_| AppError::bad("Ảnh không hợp lệ."))?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(64 * 1024 * 1024);
            limits.max_image_width = Some(4096);
            limits.max_image_height = Some(4096);
            reader.limits(limits);
            let img = reader
                .decode()
                .map_err(|_| AppError::bad("Không giải mã được ảnh."))?;
            if img.width() < 200
                || img.height() < 150
                || img.width() as u64 * img.height() as u64 > 8_000_000
            {
                return Err(AppError::bad(
                    "Ảnh cần tối thiểu 200×150 và tối đa 8 megapixel.",
                ));
            }
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png)
                .map_err(|_| AppError::bad("Không thể chuẩn hóa ảnh."))?;
            if out.get_ref().len() > 10 * 1024 * 1024 {
                return Err(AppError::bad("Ảnh sau chuẩn hóa vượt 10 MB."));
            }
            Ok((
                format!("{}.png", name.rsplit_once('.').unwrap().0),
                "image/png".into(),
                out.into_inner(),
            ))
        }
        "c" | "h" | "txt" | "md" | "kicad_sch" | "kicad_pcb" | "kicad_pro" | "step" | "stp" => {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| AppError::bad("Tệp văn bản phải dùng UTF-8."))?;
            if text.contains('\0') || text.starts_with("MZ") {
                return Err(AppError::bad("Không nhận tệp thực thi."));
            }
            if ["step", "stp"].contains(&ext.as_str())
                && !text.trim_start().starts_with("ISO-10303-21")
            {
                return Err(AppError::bad("Tệp STEP không hợp lệ."));
            }
            if ["kicad_sch", "kicad_pcb"].contains(&ext.as_str())
                && !text.trim_start().starts_with("(kicad")
            {
                return Err(AppError::bad("Tệp KiCad không hợp lệ."));
            }
            if ext == "kicad_pro" && serde_json::from_str::<Value>(text).is_err() {
                return Err(AppError::bad("Project KiCad không hợp lệ."));
            }
            Ok((
                name.into(),
                "text/plain; charset=utf-8".into(),
                bytes.to_vec(),
            ))
        }
        "pdf" if bytes.starts_with(b"%PDF-") => {
            Ok((name.into(), "application/pdf".into(), bytes.to_vec()))
        }
        "sldprt" | "sldasm" | "slddrw"
            if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) =>
        {
            Ok((
                name.into(),
                "application/octet-stream".into(),
                bytes.to_vec(),
            ))
        }
        "stl"
            if bytes.starts_with(b"solid ")
                || (bytes.len() >= 84
                    && u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize * 50
                        + 84
                        == bytes.len()) =>
        {
            Ok((
                name.into(),
                "application/octet-stream".into(),
                bytes.to_vec(),
            ))
        }
        _ => Err(AppError::bad(
            "Loại tệp không được hỗ trợ hoặc nội dung không khớp phần mở rộng.",
        )),
    }
}
async fn upload(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    mut multipart: Multipart,
) -> Result<Json<Value>> {
    let u = v.learn()?;
    let field = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad("Không đọc được tệp tải lên."))?
        .ok_or_else(|| AppError::bad("Chọn một tệp."))?;
    if field.name() != Some("file") {
        return Err(AppError::bad("Trường tải lên phải là file."));
    }
    let name = field
        .file_name()
        .ok_or_else(|| AppError::bad("Thiếu tên tệp."))?
        .to_string();
    let bytes = field
        .bytes()
        .await
        .map_err(|_| AppError::bad("Tệp vượt giới hạn tải lên."))?;
    if multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad("Dữ liệu tải lên không hợp lệ."))?
        .is_some()
    {
        return Err(AppError::bad("Mỗi lần tải đúng một tệp."));
    }
    let _permit = s.hashes.acquire().await.unwrap();
    let (name, mime, bytes) = tokio::task::spawn_blocking(move || validate_file(&name, &bytes))
        .await
        .map_err(|_| AppError::bad("Không xử lý được tệp."))??;
    let id = Uuid::new_v4();
    let sha = hex::encode(Sha256::digest(&bytes));
    let size = bytes.len() as i64;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    tx.query_one("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&u.id])
        .await?;
    let pending:i64=tx.query_one("SELECT count(*) FROM files WHERE owner_id=$1 AND submission_id IS NULL AND lesson_id IS NULL",&[&u.id]).await?.get(0);
    if pending >= 32 {
        return Err(AppError::bad(
            "Bạn có quá nhiều tệp chưa nộp. Xóa tệp không dùng trước.",
        ));
    }
    let path = s.config.data.join("uploads").join(id.to_string());
    tokio::fs::create_dir_all(path.parent().unwrap()).await?;
    tokio::fs::write(&path, &bytes).await?;
    let inserted=tx.execute("INSERT INTO files(id,owner_id,name,storage_key,content_type,size,sha256) VALUES($1,$2,$3,$1,$4,$5,$6)",&[&id,&u.id,&name,&mime,&size,&sha]).await;
    if let Err(e) = inserted {
        let _ = tokio::fs::remove_file(path).await;
        return Err(e.into());
    }
    tx.commit().await?;
    Ok(Json(
        json!({"id":id,"name":name,"size":size,"contentType":mime,"sha256":sha}),
    ))
}
async fn download(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let u = v.auth()?;
    let c = s.pool.get().await?;
    let r=c.query_opt("SELECT owner_id,name,storage_key,content_type,submission_id,lesson_track,lesson_id FROM files WHERE id=$1",&[&id]).await?.ok_or_else(||AppError::missing("Không tìm thấy tệp."))?;
    if let (Some(track), Some(lesson)) = (
        r.get::<_, Option<String>>("lesson_track"),
        r.get::<_, Option<String>>("lesson_id"),
    ) {
        catalog::readable(&s, &v, &track, &lesson).await?;
    } else {
        if u.role != "admin" && r.get::<_, Uuid>("owner_id") != u.id {
            return Err(AppError::missing("Không tìm thấy tệp."));
        }
        v.learn()?;
    }
    let bytes = tokio::fs::read(
        s.config
            .data
            .join("uploads")
            .join(r.get::<_, Uuid>("storage_key").to_string()),
    )
    .await?;
    attachment(
        bytes,
        &r.get::<_, String>("name"),
        &r.get::<_, String>("content_type"),
        false,
    )
}
async fn remove(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let u = v.auth()?;
    let mut c = s.pool.get().await?;
    let tx = c.transaction().await?;
    let r = tx
        .query_opt(
            "SELECT owner_id,storage_key,submission_id,lesson_id FROM files WHERE id=$1 FOR UPDATE",
            &[&id],
        )
        .await?
        .ok_or_else(|| AppError::missing("Không tìm thấy tệp."))?;
    if r.get::<_, Option<Uuid>>("submission_id").is_some() {
        return Err(AppError::conflict(
            "Tệp thuộc bài nộp; lịch sử bằng chứng được giữ nguyên.",
        ));
    }
    if r.get::<_, Uuid>("owner_id") != u.id && u.role != "admin" {
        return Err(AppError::missing("Không tìm thấy tệp."));
    }
    tx.execute("DELETE FROM files WHERE id=$1", &[&id]).await?;
    db::audit(&tx, Some(u.id), "file.deleted", &id.to_string(), json!({})).await?;
    tx.commit().await?;
    let path = s
        .config
        .data
        .join("uploads")
        .join(r.get::<_, Uuid>("storage_key").to_string());
    if let Err(e) = tokio::fs::remove_file(path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("Removed file metadata; orphan storage awaits maintenance");
    }
    Ok(Json(json!({"message":"Đã xóa tệp."})))
}
pub fn attachment(bytes: Vec<u8>, name: &str, mime: &str, inline: bool) -> Result<Response> {
    let encoded: String = url::form_urlencoded::byte_serialize(name.as_bytes()).collect();
    let mut response = Body::from(bytes).into_response();
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        mime.parse()
            .map_err(|_| AppError::bad("Loại tệp không hợp lệ."))?,
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        format!(
            "{}; filename*=UTF-8''{}",
            if inline { "inline" } else { "attachment" },
            encoded.replace('+', "%20")
        )
        .parse()
        .unwrap(),
    );
    Ok(response)
}
async fn handbook(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Response> {
    v.learn()?;
    catalog::valid_track(&track)?;
    let c = s.pool.get().await?;
    let status: String = c
        .query_one("SELECT status FROM courses WHERE category=$1", &[&track])
        .await?
        .get(0);
    if status != "published" && v.admin().is_err() {
        return Err(AppError::forbidden("Giáo trình chưa được mở."));
    }
    let file = match track.as_str() {
        "embedded" => "EP-LAB-Giao-trinh-C-nhung-v6.pdf",
        "kicad" => "EP-LAB-Giao-trinh-KiCad-v6.pdf",
        _ => "EP-LAB-Giao-trinh-SOLIDWORKS-v6.pdf",
    };
    attachment(
        tokio::fs::read(std::path::Path::new("content/handbooks/classroom-handbooks").join(file))
            .await?,
        file,
        "application/pdf",
        true,
    )
}
async fn host_handbook(
    Extension(v): Extension<Viewer>,
    Path(track): Path<String>,
) -> Result<Response> {
    v.admin()?;
    catalog::valid_track(&track)?;
    let file = match track.as_str() {
        "embedded" => "EP-LAB-C-nhung-Nguoi-chu-tri.pdf",
        "kicad" => "EP-LAB-KiCad-Nguoi-chu-tri.pdf",
        _ => "EP-LAB-SOLIDWORKS-Nguoi-chu-tri.pdf",
    };
    attachment(
        tokio::fs::read(std::path::Path::new("content/handbooks/host-handbooks").join(file))
            .await?,
        file,
        "application/pdf",
        true,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_executables_and_path_injection() {
        assert!(validate_file("../bad.c", b"int x;").is_err());
        assert!(validate_file("a.c", b"MZbinary").is_err());
        assert!(validate_file("a.svg", b"<svg/>").is_err());
        assert!(validate_file("a.pdf", b"not pdf").is_err());
        assert!(validate_file("a.exe", b"MZ").is_err());
        assert!(validate_file("main.c", b"int main(void) { return 0; }").is_ok());
    }
}
