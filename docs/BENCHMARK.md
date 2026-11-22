# So sánh EP Lab TypeScript và Rust

Đây là phép đo **hai ứng dụng cụ thể**, không chứng minh Rust luôn nhanh hơn TypeScript. Bản mới có kiến trúc và phạm vi chức năng khác; xem [đối chiếu chức năng](FEATURES.md).

Thời điểm đo: 2026-11-22T17:57:29.971Z (UTC). Dữ liệu thô: [lượt cuối](benchmark-results.json), [trước tối ưu](benchmark-before-optimization.json). Không loại bỏ các kết quả mà Rust chậm hơn.

## Thiết lập

- AMD Ryzen 7 6800HS with Radeon Graphics, 16 CPU logic, 15.3 GiB RAM, win32 10.0.26200.
- rustc 1.100.0 (0ef1bc386 2026-11-09); Node v24.16.0, Fastify 5.12.8, Next 16.4.6, PostgreSQL 18.6.
- Bản gốc tại commit [a268eb5](https://github.com/ekt0810/EPlab/commit/a268eb5776523d6def6366ad70c754ad646bc7c9). Fastify chạy JavaScript đã build, Next chạy next start; Rust chạy release, thin LTO.
- Cùng cluster PostgreSQL local, database tách biệt; pool 8 kết nối mỗi ứng dụng; cùng 1.000 học viên giả lập, quyền thành viên và 3 dòng tiến độ/người; chạy ANALYZE trước đo.
- Cùng 100 bài học. Đăng nhập dùng cùng Argon2id (19.456 KiB, t=2, p=1). Không bỏ kiểm quyền, session hay rate limit. Các session benchmark được tạo trong DB test, mật khẩu/hash không nằm trong báo cáo.
- Bốn workload API, concurrency 1/8/32, mỗi mẫu 600 request, lặp 3 lượt, đổi thứ tự ứng dụng. Warm-up 100 request/workload/ứng dụng. HTTP keep-alive, không nén ở cả hai API. Đọc hết body và xác nhận JSON đúng trước tính thành công.

**43,200 request API đo chính; 0 phản hồi lỗi hoặc sai dữ liệu.** Không tính warm-up, login và tải trang vào con số này.

## API

Các giá trị là trung vị của 3 lượt. Cột p95 là trung vị của p95 từng lượt; không phải p95 gộp. Tỷ lệ >1 nghĩa là Rust có throughput cao hơn.

| Workload | Đồng thời | TS req/s | Rust req/s | Rust / TS | TS p95 ms | Rust p95 ms |
|---|---:|---:|---:|---:|---:|---:|
|Danh sách môn|1|869|725|0.83×|1.55|1.70|
|Danh sách bài trong môn|1|652|448|0.69×|1.92|2.64|
|Chi tiết bài học|1|546|1,445|2.65×|2.27|0.88|
|Tiến độ cá nhân|1|759|2,297|3.03×|1.65|0.58|
|Danh sách môn|8|4,133|5,408|1.31×|2.42|1.80|
|Danh sách bài trong môn|8|2,295|2,836|1.24×|4.50|3.44|
|Chi tiết bài học|8|2,131|7,546|3.54×|4.28|1.32|
|Tiến độ cá nhân|8|3,043|11,212|3.68×|3.10|0.85|
|Danh sách môn|32|3,937|5,571|1.42×|9.87|6.35|
|Danh sách bài trong môn|32|2,179|2,925|1.34×|18.36|11.82|
|Chi tiết bài học|32|2,101|7,558|3.60×|19.67|4.97|
|Tiến độ cá nhân|32|2,874|10,765|3.75×|14.78|3.42|

Khi chỉ một request chạy tại một thời điểm, Rust vẫn chậm hơn ở danh sách môn/danh sách bài. Mã hiện tại đọc học liệu có thể chỉnh sửa trong PostgreSQL; bản gốc giữ phần lớp học trong module TypeScript. Đây là khác biệt công việc cần làm, không chỉ khác ngôn ngữ. Bài học và tiến độ ở tải đồng thời cho kết quả tốt hơn trong cấu hình đo này.

## Trình duyệt và bộ nhớ

Edge/Chromium 158.0.4370.33, viewport 1365×900. Cùng bài lesson-detail (cb01), phiên học viên, context mới mỗi lượt để tránh cache trình duyệt. Server đã warm-up. Chặn tài nguyên ngoài origin ở cả hai để tách biến động mạng/video. Lấy 5 mẫu sau một lượt warm-up, đổi thứ tự mỗi lượt.

| Chỉ số (trung vị) | TypeScript | Rust |
|---|---:|---:|
|Thời gian nội dung bài xuất hiện|221.42 ms|149.34 ms|
|TTFB trang HTML|14.10 ms|6.70 ms|
|First Contentful Paint|80.00 ms|116.00 ms|
|Tổng byte truyền cùng origin (có nén)|235.99 KiB|14.48 KiB|
|JavaScript sau giải nén|671.61 KiB|10.81 KiB|
|Số request cùng origin|17|5|
|Working set sau tải API và trình duyệt|197.57 MiB|19.55 MiB|

TypeScript gồm API 101.37 MiB + Next 96.20 MiB; Rust là một process. Working set **không gồm PostgreSQL, trình duyệt, công cụ đo**, không phải dung lượng cài đặt hoặc mức RAM tối đa. Mức giảm trong snapshot này: 90.1%.

Trang Rust dựng HTML ở server và dùng JavaScript thuần; bản React có nhiều tính năng trình học hơn. Số byte và tốc độ hiển thị không đại diện cho hai giao diện giống hệt nhau. Không đo mobile CPU throttling, mạng WAN hay CDN.

## CPU của API

CPU kernel + user đo từ hệ điều hành; trung vị ở concurrency 32, quy đổi ms CPU/1.000 request. PostgreSQL và load generator nằm ngoài số này. Bộ đếm CPU Windows có bước lượng tử; mẫu ngắn nên chỉ xem như chỉ báo.

| Workload | TS CPU ms/1.000 req | Rust CPU ms/1.000 req |
|---|---:|---:|
|Danh sách môn|260.4|234.4|
|Danh sách bài trong môn|442.7|677.1|
|Chi tiết bài học|520.8|260.4|
|Tiến độ cá nhân|364.6|130.2|

## Đăng nhập thật

Chỉ 8 mẫu/mức đồng thời để giữ nguyên hạn mức 20 lần/15 phút của bản gốc. Cỡ mẫu nhỏ, không dùng để tuyên bố khả năng phục vụ đăng nhập quy mô lớn.

| Ứng dụng | Đồng thời | Mẫu | p50 ms | p95 ms |
|---|---:|---:|---:|---:|
|typescript|1|8|31.13|41.65|
|typescript|8|8|52.86|82.53|
|rust|1|8|25.87|33.30|
|rust|8|8|29.65|57.93|

## Tối ưu được kiểm chứng

Lượt đầu phát hiện đọc thừa toàn bộ JSON bài học và chuẩn bị lại truy vấn mỗi request. Bản cuối chỉ lấy các trường cần cho danh sách, tránh clone cả khóa lessons, và dùng cache prepared statement của pool. Quyền và dữ liệu vẫn đọc mới mỗi request. Kiểm thử thay đổi trạng thái/thu hồi quyền vẫn chạy sau tối ưu.

| Workload Rust, concurrency 8 | Trước req/s | Sau req/s |
|---|---:|---:|
|Danh sách môn|3,780|5,408|
|Danh sách bài trong môn|1,856|2,836|
|Chi tiết bài học|3,842|7,546|
|Tiến độ cá nhân|5,476|11,212|

Đây là các phép đo trước và sau tối ưu trên cùng máy, có biến động nền. Không chọn riêng request tốt nhất; cả hai bộ dữ liệu đều được lưu. Thông lượng ngắn trên loopback không dự báo tải thực tế kéo dài. Chưa kiểm tải nhiều giờ, hàng triệu bản ghi, email SMTP ngoài, AI ngoài hay lượng upload lớn.

## Chạy lại

1. Checkout bản TypeScript đúng commit, npm ci, build production với EP_API_URL=http://127.0.0.1:4201. Cổng 4201 phải trống.
2. Trong repository Rust: cargo build --release --locked, npm ci, npm run db:start; cài Python 3 và Edge (Windows) hoặc Chromium của Playwright (Linux).
3. Đặt BASELINE_ROOT tới checkout TypeScript và đảm bảo rustc có trong PATH; chạy npm run bench.
4. Chạy node scripts/benchmark-report.mjs để tạo lại báo cáo. Harness tạo/xóa database có tên ngẫu nhiên, không dùng database thật của hai ứng dụng.

Binary Rust được đo có SHA-256: 9fb668c99db55c1c97081f077b5878950cc7cc9568aa2f0a41719acfabd6580d.
