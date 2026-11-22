use super::*;
use crate::{AppState, db, error::Result, security::Viewer};
use axum::{Extension, extract::State, response::Html};
use maud::html;
pub async fn login(Extension(v): Extension<Viewer>) -> Html<String> {
    page(
        "Đăng nhập",
        "/dang-nhap",
        &v,
        html! {section class="auth-panel panel"{(heading("CHÀO MỪNG TRỞ LẠI","Đăng nhập","Tiếp tục việc học và thực hành của bạn."))form class="stack" data-api="/api/auth/login"{(input("Email","email","email","",true))(input("Mật khẩu","password","password","",true))(submit("Đăng nhập"))}div class="auth-links"{a href="/quen-mat-khau"{"Quên mật khẩu?"}a href="/dang-ky"{"Tạo tài khoản"}}}},
    )
}
pub async fn register(Extension(v): Extension<Viewer>) -> Html<String> {
    page(
        "Tạo tài khoản",
        "/dang-ky",
        &v,
        html! {section class="auth-panel panel"{(heading("THAM GIA EP LAB","Tạo tài khoản","Xác minh email trước khi đăng nhập. Mật khẩu từ 12 đến 128 ký tự."))form class="stack" data-api="/api/auth/register"{(input("Họ và tên","name","text","",true))(input("Email","email","email","",true))(input("Số điện thoại","phone","tel","",true))(input("Mật khẩu","password","password","",true))(submit("Tạo tài khoản"))}div class="auth-links"{a href="/dang-nhap"{"Đã có tài khoản? Đăng nhập"}}}},
    )
}
pub async fn verify(Extension(v): Extension<Viewer>) -> Html<String> {
    page(
        "Xác minh email",
        "/xac-minh",
        &v,
        html! {section class="auth-panel panel"{(heading("HOÀN TẤT ĐĂNG KÝ","Xác minh email","Mở liên kết trong email rồi bấm nút bên dưới để xác nhận."))form class="stack" data-api="/api/auth/verify" data-token-form{(hidden("token",""))(submit("Xác minh email"))}details{summary{"Gửi lại liên kết"}form class="stack" data-api="/api/auth/resend"{(input("Email","email","email","",true))(submit("Gửi lại email"))}}}},
    )
}
pub async fn forgot(Extension(v): Extension<Viewer>) -> Html<String> {
    page(
        "Quên mật khẩu",
        "/quen-mat-khau",
        &v,
        html! {section class="auth-panel panel"{(heading("TÀI KHOẢN","Khôi phục mật khẩu","Nhập email của bạn để nhận liên kết khôi phục."))form class="stack" data-api="/api/auth/forgot"{(input("Email","email","email","",true))(submit("Gửi liên kết"))}a href="/dang-nhap"{"← Về đăng nhập"}}},
    )
}
pub async fn reset(Extension(v): Extension<Viewer>) -> Html<String> {
    password_page(&v, false)
}
pub async fn invite(Extension(v): Extension<Viewer>) -> Html<String> {
    password_page(&v, true)
}
fn password_page(v: &Viewer, invite: bool) -> Html<String> {
    page(
        "Thiết lập mật khẩu",
        "",
        v,
        html! {section class="auth-panel panel"{(heading("TÀI KHOẢN",if invite{"Nhận tài khoản EP Lab"}else{"Đặt lại mật khẩu"},"Mật khẩu từ 12 đến 128 ký tự. Liên kết chỉ sử dụng được một lần."))form class="stack" data-api=(if invite{"/api/auth/accept-invitation"}else{"/api/auth/reset"}) data-token-form{(hidden("token",""))(input("Mật khẩu mới","password","password","",true))@if invite{label class="check"{input type="checkbox" name="acceptedTerms" required;span{"Tôi đồng ý dùng tài khoản cá nhân để học và thực hành trong Lab."}}}(submit("Lưu mật khẩu"))}}},
    )
}
pub async fn profile(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Html<String>> {
    let u = match v.auth() {
        Ok(u) => u,
        Err(e) => return gate("Tài khoản", "/tai-khoan", &v, e),
    };
    let c = s.pool.get().await?;
    let sessions=db::values(&c,"SELECT jsonb_build_object('id',id,'user_agent',user_agent,'created_at',created_at,'current',id=$2) FROM sessions WHERE user_id=$1 AND expires_at>now() ORDER BY created_at DESC",&[&u.id,&u.session_id]).await?;
    Ok(page(
        "Tài khoản",
        "/tai-khoan",
        &v,
        html! {(heading("HỒ SƠ CÁ NHÂN",&u.name,&u.email))
        div class="two-column"{section class="panel article"{h2{"Thông tin liên hệ"}form class="stack" data-api="/api/auth/profile" data-method="PATCH" data-reload="true"{(input("Họ và tên","name","text",&u.name,true))(input("Số điện thoại","phone","tel",&u.phone,true))(submit("Lưu thông tin"))}}
        section class="panel article"{h2{"Đổi mật khẩu"}form class="stack" data-api="/api/auth/password" data-method="POST"{(input("Mật khẩu hiện tại","current","password","",true))(input("Mật khẩu mới","password","password","",true))p class="muted"{"Thao tác này đăng xuất tất cả phiên hiện có."}(submit("Đổi mật khẩu"))}}}
        section class="panel article"{h2{"Phiên đăng nhập"}@for session in sessions{div class="list-row"{div{strong{(if session["current"]==true{"Phiên hiện tại"}else{"Thiết bị khác"})}p class="muted wrap"{(text(&session,"user_agent"))}small{(date(&session["created_at"]))}}form data-api=(format!("/api/auth/sessions/{}",text(&session,"id"))) data-method="DELETE" data-reload="true"{button class="button"{"Đăng xuất phiên"}}}}}
        },
    ))
}
pub async fn membership(
    State(s): State<AppState>,
    Extension(v): Extension<Viewer>,
) -> Result<Html<String>> {
    if let Err(e) = v.auth() {
        return gate("Tham gia Lab", "/tham-gia-lab", &v, e);
    }
    let d = crate::membership::current_data(&s, &v).await?;
    let r = &d["request"];
    Ok(page(
        "Tham gia Lab",
        "/tham-gia-lab",
        &v,
        html! {
        (heading("CÙNG HỌC, CÙNG LÀM","Tham gia EP Lab","Miễn phí. Một lần duyệt mở đầy đủ nội dung, thực hành và trao đổi của ba môn."))
        section class="panel article narrow"{@if d["active"]==true{span class="badge success"{"Đã là thành viên"}h2{"Bạn đã có quyền học đầy đủ."}p{"Chọn một môn và tiếp tục lộ trình của bạn."}a class="button primary" href="/lop-hoc/embedded"{"Vào lớp học"}}
        @else if r["status"]=="pending"{span class="badge"{"Chờ người chủ trì duyệt"}h2{"Yêu cầu đã được gửi."}p class="prose"{(text(r,"message"))}p class="muted"{"Kết quả duyệt sẽ xuất hiện tại đây."}}
        @else{@if !r.is_null(){div class="notice"{strong{(status(text(r,"status")))}p{(text(r,"decision_note"))}}}
        form class="stack" data-api="/api/lab/membership"{fieldset{legend{"Bạn quan tâm môn nào?"}@for (track,label) in [("embedded","Lập trình nhúng C"),("kicad","Thiết kế mạch KiCad"),("solidworks","Thiết kế cơ khí SOLIDWORKS")]{label class="check"{input type="checkbox" name="tracks[]" value=(track);span{(label)}}}}
        (area("Mục tiêu học tập","message",text(r,"message"),true,4))p class="muted"{"Mô tả điều bạn muốn làm trong ít nhất 10 ký tự."}(submit("Gửi yêu cầu tham gia"))}}
        }
        },
    ))
}
