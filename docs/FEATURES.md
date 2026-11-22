# Đối chiếu chức năng

Bản gốc: ekt0810/EPlab tại commit a268eb5776523d6def6366ad70c754ad646bc7c9.
Bản Rust là website độc lập cho mô hình Lab miễn phí, không phải port nhị phân hay bản di trú database.

| Nhóm                                    | Bản Rust 0.1 | Ghi chú                                                                                         |
| --------------------------------------- | ------------ | ----------------------------------------------------------------------------------------------- |
| Tài khoản học viên                      | Có           | Đăng ký/xác minh/khôi phục/đổi mật khẩu, profile, phiên                                         |
| Người chủ trì                           | Có           | Tối đa hai vị trí, tạo qua CLI                                                                  |
| Duyệt quyền học                         | Có           | Duyệt một lần bấm; kiểm tra revision; phản hồi khi từ chối/thu hồi                              |
| Quản lý học viên                        | Có           | Tìm/lọc/phân trang, thư mời, ghi chú, khóa/mở khóa, thu hồi phiên                               |
| Nhật ký                                 | Có           | Thao tác quản lý, quyền học, chấm bài, tài khoản                                                |
| 100 bài học hiện tại                    | Có           | 32 nhúng C, 44 KiCad, 24 SOLIDWORKS                                                             |
| Giáo trình, nguồn video                 | Có           | PDF có kiểm quyền; liên kết video, không tải/copy video                                         |
| Tiến độ và tự kiểm                      | Có           | Đáp án được kiểm ở server; phải hoàn thành checklist và quiz                                    |
| Tra cứu giáo trình                      | Có           | Tìm đoạn liên quan có nguồn, lưu lịch sử, lọc lại quyền nguồn                                   |
| AI ngoài hệ thống                       | Chưa         | Không gọi mô hình trả phí; không giả gọi AI                                                     |
| Biên tập lớp học                        | Có           | Tiêu đề, khái niệm, mục tiêu, bước thực hành, quiz, trạng thái, tệp                             |
| Bài thực hành                           | Có, 36 bài   | Bộ đề export ban đầu; các mở rộng KiCad cài bằng install-pathways của bản cũ chưa chuyển đầy đủ |
| Giao, nộp và chấm bài                   | Có           | Cá nhân/cả lớp; 1–2 ảnh, tối đa 6 tệp nguồn; chấm thủ công theo rubric                          |
| Chấm AI tự động                         | Chưa         | Kết quả do người chủ trì đánh giá; server tính điểm                                             |
| Trao đổi riêng                          | Có           | Mở/đóng chủ đề, trả lời, chống gửi trùng                                                        |
| Cuộc thi                                | Có           | Người chủ trì nhập nguồn chính thức và ẩn/hiện; không có crawler                                |
| MFA/TOTP, mã dự phòng                   | Chưa         | Giữ bản cũ nếu cần tính năng này                                                                |
| Thanh toán/webhook cũ                   | Chưa         | Bản này dùng quyền Lab miễn phí                                                                 |
| S3 và media riêng                       | Chưa         | Lưu tệp local với kiểm quyền                                                                    |
| Trình học theo từng cảnh, đọc giọng nói | Khác         | Rust dùng trang bài học có các phần; không chuyển hệ cảnh/giọng đọc của React                   |
| Vận hành Windows/ngrok tự động          | Chưa         | Chạy executable; không thay dịch vụ đang hoạt động                                              |
| Backup/restore có xác minh tự động      | Chưa         | Phải sao lưu database và DATA_DIR riêng                                                         |
| Di trú tài khoản/bài nộp thật           | Chưa         | Không tự đọc, nhập hay sửa dữ liệu thật của hệ thống cũ                                         |
| Giao diện                               | Mới          | HTML SSR, CSS/JS thuần, desktop/mobile, sáng/tối                                                |

Điểm số, dữ liệu và phân quyền được kiểm thử ở API và trình duyệt. Những mục “chưa” là giới hạn thực tế, không phải tính năng ẩn đã làm xong. Benchmark chỉ so sánh các luồng tương ứng đã hiện thực ở cả hai bản.
