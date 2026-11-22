import fs from "node:fs/promises";
const r = JSON.parse(await fs.readFile("docs/benchmark-results.json", "utf8"));
const before = JSON.parse(
  await fs.readFile("docs/benchmark-before-optimization.json", "utf8"),
);
const median = (a) => [...a].sort((x, y) => x - y)[Math.floor(a.length / 2)];
const n = (value, d = 2) =>
  value.toLocaleString("en-US", {
    maximumFractionDigits: d,
    minimumFractionDigits: d,
  });
const group = (data, impl, work, c) =>
  data.samples.filter(
    (s) =>
      s.implementation === impl && s.workload === work && s.concurrency === c,
  );
const metric = (data, impl, work, c, key) =>
  median(group(data, impl, work, c).map((s) => s[key]));
const names = {
  "course-index": "Danh sách môn",
  "course-detail": "Danh sách bài trong môn",
  "lesson-detail": "Chi tiết bài học",
  "personal-progress": "Tiến độ cá nhân",
};
const out = [];
out.push("# So sánh EP Lab TypeScript và Rust\n");
out.push(
  "Đây là phép đo **hai ứng dụng cụ thể**, không chứng minh Rust luôn nhanh hơn TypeScript. Bản mới có kiến trúc và phạm vi chức năng khác; xem [đối chiếu chức năng](FEATURES.md).\n",
);
out.push(
  "Thời điểm đo: " +
    r.date +
    " (UTC). Dữ liệu thô: [lượt cuối](benchmark-results.json), [trước tối ưu](benchmark-before-optimization.json). Không loại bỏ các kết quả mà Rust chậm hơn.\n",
);
out.push("## Thiết lập\n");
out.push(
  "- " +
    r.hardware.cpu.trim() +
    ", " +
    r.hardware.logicalCpus +
    " CPU logic, " +
    n(r.hardware.ramGiB, 1) +
    " GiB RAM, " +
    r.hardware.os +
    ".\n- " +
    r.versions.rust +
    "; Node " +
    r.versions.node +
    ", Fastify " +
    r.versions.fastify +
    ", Next " +
    r.versions.next +
    ", PostgreSQL " +
    r.versions.postgres +
    ".\n- Bản gốc tại commit [" +
    r.versions.baselineCommit.slice(0, 7) +
    "](https://github.com/ekt0810/EPlab/commit/" +
    r.versions.baselineCommit +
    "). Fastify chạy JavaScript đã build, Next chạy next start; Rust chạy release, thin LTO.\n- Cùng cluster PostgreSQL local, database tách biệt; pool 8 kết nối mỗi ứng dụng; cùng 1.000 học viên giả lập, quyền thành viên và 3 dòng tiến độ/người; chạy ANALYZE trước đo.\n- Cùng 100 bài học. Đăng nhập dùng cùng Argon2id (19.456 KiB, t=2, p=1). Không bỏ kiểm quyền, session hay rate limit. Các session benchmark được tạo trong DB test, mật khẩu/hash không nằm trong báo cáo.\n- Bốn workload API, concurrency 1/8/32, mỗi mẫu 600 request, lặp 3 lượt, đổi thứ tự ứng dụng. Warm-up 100 request/workload/ứng dụng. HTTP keep-alive, không nén ở cả hai API. Đọc hết body và xác nhận JSON đúng trước tính thành công.\n",
);
out.push(
  "**" +
    r.samples.reduce((a, s) => a + s.requests, 0).toLocaleString("en-US") +
    " request API đo chính; " +
    r.samples.reduce((a, s) => a + s.errors, 0) +
    " phản hồi lỗi hoặc sai dữ liệu.** Không tính warm-up, login và tải trang vào con số này.\n",
);
out.push(
  "## API\n\nCác giá trị là trung vị của 3 lượt. Cột p95 là trung vị của p95 từng lượt; không phải p95 gộp. Tỷ lệ >1 nghĩa là Rust có throughput cao hơn.\n",
);
out.push(
  "| Workload | Đồng thời | TS req/s | Rust req/s | Rust / TS | TS p95 ms | Rust p95 ms |\n|---|---:|---:|---:|---:|---:|---:|",
);
for (const c of [1, 8, 32])
  for (const [work, name] of Object.entries(names)) {
    const ts = metric(r, "typescript", work, c, "rps"),
      rs = metric(r, "rust", work, c, "rps");
    out.push(
      "|" +
        name +
        "|" +
        c +
        "|" +
        n(ts, 0) +
        "|" +
        n(rs, 0) +
        "|" +
        n(rs / ts) +
        "×|" +
        n(metric(r, "typescript", work, c, "p95Ms")) +
        "|" +
        n(metric(r, "rust", work, c, "p95Ms")) +
        "|",
    );
  }
out.push(
  "\nKhi chỉ một request chạy tại một thời điểm, Rust vẫn chậm hơn ở danh sách môn/danh sách bài. Mã hiện tại đọc học liệu có thể chỉnh sửa trong PostgreSQL; bản gốc giữ phần lớp học trong module TypeScript. Đây là khác biệt công việc cần làm, không chỉ khác ngôn ngữ. Bài học và tiến độ ở tải đồng thời cho kết quả tốt hơn trong cấu hình đo này.\n",
);
out.push(
  "## Trình duyệt và bộ nhớ\n\nEdge/Chromium " +
    r.versions.browser +
    ", viewport 1365×900. Cùng bài " +
    r.samples.find((s) => s.workload === "lesson-detail")?.workload +
    " (cb01), phiên học viên, context mới mỗi lượt để tránh cache trình duyệt. Server đã warm-up. Chặn tài nguyên ngoài origin ở cả hai để tách biến động mạng/video. Lấy 5 mẫu sau một lượt warm-up, đổi thứ tự mỗi lượt.\n",
);
out.push("| Chỉ số (trung vị) | TypeScript | Rust |\n|---|---:|---:|");
for (const [key, label, unit, scale] of [
  ["lessonVisibleMs", "Thời gian nội dung bài xuất hiện", " ms", 1],
  ["ttfbMs", "TTFB trang HTML", " ms", 1],
  ["firstContentfulPaintMs", "First Contentful Paint", " ms", 1],
  ["transferBytes", "Tổng byte truyền cùng origin (có nén)", " KiB", 1024],
  ["jsDecodedBytes", "JavaScript sau giải nén", " KiB", 1024],
  ["requests", "Số request cùng origin", "", 1],
]) {
  const values = ["typescript", "rust"].map(
    (impl) =>
      n(
        median(
          r.browser.filter((b) => b.implementation === impl).map((b) => b[key]),
        ) / scale,
        key === "requests" ? 0 : 2,
      ) + unit,
  );
  out.push("|" + label + "|" + values.join("|") + "|");
}
const mem = r.wholeApplicationMemory;
out.push(
  "|Working set sau tải API và trình duyệt|" +
    n(mem.typescriptTotalRssMiB) +
    " MiB|" +
    n(mem.rustRssMiB) +
    " MiB|",
);
out.push(
  "\nTypeScript gồm API " +
    n(mem.typescriptApiRssMiB) +
    " MiB + Next " +
    n(mem.typescriptWebRssMiB) +
    " MiB; Rust là một process. Working set **không gồm PostgreSQL, trình duyệt, công cụ đo**, không phải dung lượng cài đặt hoặc mức RAM tối đa. Mức giảm trong snapshot này: " +
    n((1 - mem.rustRssMiB / mem.typescriptTotalRssMiB) * 100, 1) +
    "%.\n\nTrang Rust dựng HTML ở server và dùng JavaScript thuần; bản React có nhiều tính năng trình học hơn. Số byte và tốc độ hiển thị không đại diện cho hai giao diện giống hệt nhau. Không đo mobile CPU throttling, mạng WAN hay CDN.\n",
);
out.push(
  "## CPU của API\n\nCPU kernel + user đo từ hệ điều hành; trung vị ở concurrency 32, quy đổi ms CPU/1.000 request. PostgreSQL và load generator nằm ngoài số này. Bộ đếm CPU Windows có bước lượng tử; mẫu ngắn nên chỉ xem như chỉ báo.\n\n| Workload | TS CPU ms/1.000 req | Rust CPU ms/1.000 req |\n|---|---:|---:|",
);
for (const [work, name] of Object.entries(names)) {
  out.push(
    "|" +
      name +
      "|" +
      ["typescript", "rust"]
        .map((impl) =>
          n(
            median(
              group(r, impl, work, 32).map(
                (s) => (s.cpuMs / s.requests) * 1000,
              ),
            ),
            1,
          ),
        )
        .join("|") +
      "|",
  );
}
out.push(
  "\n## Đăng nhập thật\n\nChỉ 8 mẫu/mức đồng thời để giữ nguyên hạn mức 20 lần/15 phút của bản gốc. Cỡ mẫu nhỏ, không dùng để tuyên bố khả năng phục vụ đăng nhập quy mô lớn.\n\n| Ứng dụng | Đồng thời | Mẫu | p50 ms | p95 ms |\n|---|---:|---:|---:|---:|",
);
for (const x of r.login)
  out.push(
    "|" +
      x.implementation +
      "|" +
      x.concurrency +
      "|" +
      x.samples +
      "|" +
      n(x.p50Ms) +
      "|" +
      n(x.p95Ms) +
      "|",
  );
out.push(
  "\n## Tối ưu được kiểm chứng\n\nLượt đầu phát hiện đọc thừa toàn bộ JSON bài học và chuẩn bị lại truy vấn mỗi request. Bản cuối chỉ lấy các trường cần cho danh sách, tránh clone cả khóa lessons, và dùng cache prepared statement của pool. Quyền và dữ liệu vẫn đọc mới mỗi request. Kiểm thử thay đổi trạng thái/thu hồi quyền vẫn chạy sau tối ưu.\n\n| Workload Rust, concurrency 8 | Trước req/s | Sau req/s |\n|---|---:|---:|",
);
for (const [work, name] of Object.entries(names))
  out.push(
    "|" +
      name +
      "|" +
      n(metric(before, "rust", work, 8, "rps"), 0) +
      "|" +
      n(metric(r, "rust", work, 8, "rps"), 0) +
      "|",
  );
out.push(
  "\nĐây là các phép đo trước và sau tối ưu trên cùng máy, có biến động nền. Không chọn riêng request tốt nhất; cả hai bộ dữ liệu đều được lưu. Thông lượng ngắn trên loopback không dự báo tải thực tế kéo dài. Chưa kiểm tải nhiều giờ, hàng triệu bản ghi, email SMTP ngoài, AI ngoài hay lượng upload lớn.\n",
);
out.push(
  "## Chạy lại\n\n1. Checkout bản TypeScript đúng commit, npm ci, build production với EP_API_URL=http://127.0.0.1:4201. Cổng 4201 phải trống.\n2. Trong repository Rust: cargo build --release --locked, npm ci, npm run db:start; cài Python 3 và Edge (Windows) hoặc Chromium của Playwright (Linux).\n3. Đặt BASELINE_ROOT tới checkout TypeScript và đảm bảo rustc có trong PATH; chạy npm run bench.\n4. Chạy node scripts/benchmark-report.mjs để tạo lại báo cáo. Harness tạo/xóa database có tên ngẫu nhiên, không dùng database thật của hai ứng dụng.\n\nBinary Rust được đo có SHA-256: " +
    r.versions.rustBinarySha256 +
    ".\n",
);
await fs.writeFile("docs/BENCHMARK.md", out.join("\n"));
console.log("Wrote docs/BENCHMARK.md");
