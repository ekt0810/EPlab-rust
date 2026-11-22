# Bảo mật và vận hành

## Các cơ chế đã hiện thực

- Argon2id: memory 19,456 KiB, time cost 2, parallelism 1; cùng thông số bản TypeScript. Mật khẩu 12–128 byte, không lưu/log mật khẩu thô.
- Session ngẫu nhiên 32 byte; database chỉ giữ SHA-256 của cookie. HttpOnly, SameSite=Lax, thêm Secure khi APP_URL dùng HTTPS; tối đa 10 phiên/tài khoản.
- Token xác minh/reset/thư mời ngẫu nhiên, hash trong database, hết hạn 30 phút, dùng một lần. Đổi/reset mật khẩu thu hồi phiên.
- Mọi mutation kiểm Origin. Mutation của phiên đã đăng nhập kiểm CSRF riêng cho phiên.
- Hạn mức 180 yêu cầu/phút theo người dùng hoặc IP; đăng nhập/đăng ký/khôi phục có hạn mức IP và tài khoản lưu PostgreSQL. Không tin X-Forwarded-For.
- Duyệt thành viên, chỉnh học liệu, chấm bài dùng kiểm tra revision; thao tác quan trọng gắn audit trong transaction.
- Các truy vấn dùng tham số. Cache chỉ lưu prepared statement trên từng kết nối, không cache dữ liệu quyền hay thông tin người dùng.
- HTML escape bằng Maud. CSP chỉ cho script/style cùng origin; không script inline. Có nosniff, chống iframe, referrer policy và permissions policy.
- Quiz không gửi đáp án/explanation trước khi làm. Ghi chú/tiến độ/câu hỏi/bài nộp/trao đổi có kiểm chủ sở hữu.
- Tệp có UUID server cấp; không dùng tên client làm đường dẫn. Ảnh được giải mã, giới hạn kích thước và mã hóa lại PNG. Tệp khác được kiểm loại/nội dung cơ bản, SHA-256, quyền tải xuống.
- Bằng chứng đã nộp không thể xóa qua API tệp. Tệp đang gắn bài học phải gỡ trước khi xóa bài.
- Migration có checksum, advisory lock; seed có phiên bản riêng và không tự khôi phục dữ liệu đã xóa.

## Ranh giới hiện tại

Đây là bản 0.1 đã kiểm thử local và CI, không phải chứng nhận kiểm toán bảo mật độc lập. Chưa có MFA/TOTP, quét antivirus, PostgreSQL TLS từ xa, storage S3, hạn mức dung lượng dài hạn hay job dọn file/email cũ tự động.

Kiểm định chữ ký tệp PDF/CAD không chứng minh mọi tệp an toàn; tệp được tải dạng attachment. Quản trị hệ thống nên có quét mã độc và chính sách dung lượng nếu mở đăng ký công khai.

APP_URL công khai phải là HTTPS và MAIL_MODE=smtp. SMTP dùng TLS/STARTTLS. Cần reverse proxy cùng origin và PostgreSQL chỉ truy cập từ máy/mạng riêng. Do chưa tin proxy header, nhiều người dùng chưa đăng nhập qua cùng proxy có thể chia sẻ hạn mức IP; cần thiết kế trusted proxy trước khi triển khai rộng.

Những yêu cầu đã vào xử lý khi quản trị vừa thu hồi quyền có thể hoàn tất trước yêu cầu tiếp theo; quyền được đọc lại từ PostgreSQL ở mỗi request. Bản này không có cơ chế hủy tất cả request đang chạy.

DATA_DIR và database cần sao lưu đồng bộ; dừng nhận ghi hoặc dừng server trong lúc chụp bản sao. Khôi phục vào database/thư mục mới, kiểm tra tệp và quyền trước khi chuyển lưu lượng. Chưa có migration nhập dữ liệu từ schema TypeScript.

## Bí mật và email thử nghiệm

.env, DATA_DIR, work/, test-results/, node_modules/ và target/ không thuộc Git. Outbox local chứa liên kết bí mật và không có route HTTP để đọc. Không bật local mail cho địa chỉ công khai. Chỉ bộ thử nghiệm dùng email @example.test và dữ liệu tổng hợp.

Các ảnh trong docs/screenshots là ảnh kiểm thử với tài khoản giả; không chứa dữ liệu học viên thật.
