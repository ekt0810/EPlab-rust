import { chromium } from "playwright";
import fs from "node:fs/promises";
import assert from "node:assert/strict";
import { startEnvironment, api, root } from "./environment.mjs";
import { evidence } from "./image.mjs";
const e = await startEnvironment("browser");
await fs.mkdir(root + "/test-results", { recursive: true });
const browser = await chromium.launch({
  headless: true,
  channel: process.platform === "win32" ? "msedge" : undefined,
});
const desktop = await browser.newContext({
    viewport: { width: 1440, height: 1000 },
  }),
  learnerContext = await browser.newContext({
    viewport: { width: 1365, height: 900 },
  });
const hostPage = await desktop.newPage(),
  page = await learnerContext.newPage();
const checks = [],
  errors = [];
for (const p of [page, hostPage]) {
  p.on("pageerror", (err) => errors.push(err.message));
  p.on("console", (msg) => {
    if (
      msg.type() === "error" &&
      /Content Security Policy|Refused to/.test(msg.text())
    )
      errors.push(msg.text());
  });
}
const check = async (name, fn) => {
  await fn();
  checks.push(name);
  console.log("PASS " + name);
};
async function navigate(p, url) {
  const r = await p.goto(e.origin + url);
  assert.equal(r.status(), 200, url);
  await p.waitForLoadState("networkidle");
}
async function clickSave(p, label) {
  const button = p.getByRole("button", { name: label, exact: true });
  const endpoint = await button.evaluate((el) => {
    const form = el.closest("form");
    return {
      url: new URL(form.dataset.api, location.origin).href,
      method: form.dataset.method || "POST",
    };
  });
  const [response] = await Promise.all([
    p.waitForResponse(
      (r) => r.url() === endpoint.url && r.request().method() === endpoint.method,
    ),
    button.click(),
  ]);
  assert.equal(response.ok(), true, label + ": HTTP " + response.status());
  await p.waitForLoadState("networkidle");
}
async function login(p, email) {
  await navigate(p, "/dang-nhap");
  await p.getByLabel("Email", { exact: true }).fill(email);
  await p.getByLabel("Mật khẩu", { exact: true }).fill(e.password);
  await p.getByRole("button", { name: "Đăng nhập", exact: true }).click();
  await p.waitForURL((url) => !url.pathname.includes("dang-nhap"));
}
try {
  await check("Desktop home and three course cards", async () => {
    await navigate(page, "/");
    assert.equal(await page.locator(".course-card").count(), 3);
    await page.screenshot({
      path: root + "/test-results/home-desktop.png",
      fullPage: true,
    });
  });
  await check("Registration form submits without page reload", async () => {
    await navigate(page, "/dang-ky");
    await page.getByLabel("Họ và tên").fill("Nguyễn Minh An");
    await page
      .getByLabel("Email", { exact: true })
      .fill("browser-student@example.test");
    await page.getByLabel("Số điện thoại").fill("0900000003");
    await page.getByLabel("Mật khẩu", { exact: true }).fill(e.password);
    await clickSave(page, "Tạo tài khoản");
    await page
      .locator(".form-result")
      .filter({ hasText: "thư xác minh" })
      .waitFor();
  });
  await check("Email verification handles fragment token safely", async () => {
    const token = await e.mailToken("browser-student@example.test");
    await navigate(page, "/xac-minh#token=" + token);
    assert.equal(new URL(page.url()).hash, "");
    await page
      .getByRole("button", { name: "Xác minh email", exact: true })
      .click();
    await page.waitForURL("**/dang-nhap");
  });
  await check("Student login and profile form", async () => {
    await login(page, "browser-student@example.test");
    await navigate(page, "/tai-khoan");
    await page.getByLabel("Họ và tên").fill("Nguyễn Minh An");
    await clickSave(page, "Lưu thông tin");
    await page.getByText("Đã lưu hồ sơ.", { exact: true }).waitFor();
  });
  await check("Join Lab with multiple track selection", async () => {
    await navigate(page, "/tham-gia-lab");
    await page.getByText("Lập trình nhúng C", { exact: true }).last().click();
    await page
      .getByLabel("Mục tiêu học tập")
      .fill("Học lập trình firmware, tự kiểm thử và làm sản phẩm.");
    await clickSave(page, "Gửi yêu cầu tham gia");
    await page.getByText("Chờ người chủ trì duyệt", { exact: true }).waitFor();
  });
  await check("Admin login and pending request view", async () => {
    await login(hostPage, e.adminEmail);
    await navigate(hostPage, "/quan-tri/thanh-vien");
    await hostPage.getByRole("heading", { name: "Nguyễn Minh An" }).waitFor();
    await hostPage.screenshot({
      path: root + "/test-results/admin-pending-desktop.png",
      fullPage: true,
    });
  });
  await check("Approve in one click without typed confirmation", async () => {
    await clickSave(hostPage, "Duyệt tham gia");
    assert.equal(await hostPage.locator("#confirmation").isVisible(), false);
    await hostPage.getByText("Không có yêu cầu trong bộ lọc này.").waitFor();
    await navigate(page, "/tham-gia-lab");
    await page.getByText("Bạn đã có quyền học đầy đủ.").waitFor();
  });
  const lessons = JSON.parse(
    await fs.readFile(root + "/content/courses.json", "utf8"),
  );
  const lesson = lessons.embedded.lessons[0];
  await check("Lesson SSR, quiz and saved completion", async () => {
    await navigate(page, "/lop-hoc/embedded/" + lesson.id);
    await page
      .locator('form[data-kind="quiz"] input[type="radio"]')
      .nth(lesson.quiz.answer)
      .check();
    await clickSave(page, "Kiểm tra đáp án");
    await page.getByText("✓ Đã vượt qua tự kiểm", { exact: true }).waitFor();
    for (const box of await page.locator('input[name="checklist[]"]').all())
      await box.check();
    await page
      .getByLabel("Ghi chú riêng")
      .fill("Ghi chú được nhập từ trình duyệt.");
    await page.getByLabel("Đánh dấu hoàn thành bài").check();
    await clickSave(page, "Lưu tiến độ");
    await page.getByText("Bạn đã hoàn thành bài học.", { exact: true }).waitFor();
    await navigate(page, "/lop-hoc/embedded/" + lesson.id);
    assert.equal(
      await page.getByLabel("Ghi chú riêng").inputValue(),
      "Ghi chú được nhập từ trình duyệt.",
    );
    assert.equal(
      await page.getByLabel("Đánh dấu hoàn thành bài").isChecked(),
      true,
    );
    await page.screenshot({
      path: root + "/test-results/lesson-desktop.png",
      fullPage: true,
    });
  });
  await check("Handbook search returns source citation", async () => {
    await page.getByLabel("Bạn muốn hiểu thêm điều gì?").fill(lesson.title);
    await clickSave(page, "Tìm trong giáo trình");
    await page.locator(".answer").first().waitFor();
    assert.ok(await page.locator(".answer a").count());
  });
  const client = api(e.origin);
  await client.call("POST", "/api/auth/login", {
    email: e.adminEmail,
    password: e.password,
  });
  const exercises = (
    await client.call("GET", "/api/practice/embedded/exercises")
  ).data;
  const exercise = (
    await client.call(
      "GET",
      "/api/practice/embedded/exercises/" + exercises[0].id,
    )
  ).data;
  await check("Assignment can be created from admin form", async () => {
    await navigate(hostPage, "/quan-tri/thuc-hanh/embedded");
    await hostPage.getByText("Giao bài thực hành", { exact: true }).click();
    await hostPage.locator('[name="exerciseId"]').selectOption(exercise.id);
    await hostPage
      .getByLabel("Hướng dẫn bổ sung")
      .fill("Kiểm tra chương trình và gửi ảnh kết quả.");
    await clickSave(hostPage, "Giao bài");
    await hostPage
      .getByText("Đã giao bài thực hành.", { exact: true })
      .waitFor();
  });
  let submissionURL;
  await check("Browser uploads real PNG and submits practice", async () => {
    await navigate(page, "/thuc-hanh/embedded/" + exercise.id);
    await page
      .getByLabel("Mô tả cách làm và kết quả (ít nhất 15 ký tự)")
      .fill("Em đã chạy chương trình và lưu ảnh bằng chứng.");
    await page
      .locator("[data-upload]")
      .setInputFiles({
        name: "firmware-evidence.png",
        mimeType: "image/png",
        buffer: evidence(),
      });
    await page.locator(".upload-item").waitFor();
    await page
      .getByRole("button", { name: "Nộp bài để nhận phản hồi" })
      .click();
    await page.waitForURL("**/bai-nop/*");
    submissionURL = new URL(page.url()).pathname;
    await page.getByText("Đang chờ người chủ trì xem và chấm bài.").waitFor();
  });
  await check("Host reviews all criteria through UI", async () => {
    await navigate(hostPage, submissionURL);
    await hostPage
      .getByLabel("Nhận xét chung", { exact: true })
      .fill("Bài thực hành đạt yêu cầu theo bằng chứng.");
    const form = hostPage.locator('form[data-api$="/review"]');
    for (const sel of await form.locator("select").all())
      await sel.selectOption("met");
    for (const area of await form
      .getByLabel("Bằng chứng đánh giá", { exact: true })
      .all())
      await area.fill("Ảnh minh họa đầy đủ tiêu chí này.");
    await clickSave(hostPage, "Lưu điểm & phản hồi");
    await hostPage.getByText("100 / 100", { exact: true }).waitFor();
    await navigate(page, submissionURL);
    await page.getByText("100 / 100", { exact: true }).waitFor();
  });
  let threadURL;
  await check("Student creates a private conversation", async () => {
    await navigate(page, "/trao-doi/embedded");
    await page.getByText("Mở chủ đề trao đổi", { exact: true }).click();
    await page
      .getByLabel("Chủ đề", { exact: true })
      .fill("Cách kiểm tra firmware");
    await page
      .getByLabel("Nội dung", { exact: true })
      .fill("Em muốn kiểm tra trường hợp lỗi khi nhận dữ liệu.");
    await page.getByRole("button", { name: "Gửi trao đổi" }).click();
    await page.waitForURL("**/trao-doi/chu-de/*");
    threadURL = new URL(page.url()).pathname;
  });
  await check("Host replies and closes conversation", async () => {
    await navigate(hostPage, threadURL);
    await hostPage
      .getByLabel("Tin nhắn", { exact: true })
      .fill("Hãy tạo test case dữ liệu bị thiếu và vượt kích thước.");
    await clickSave(hostPage, "Gửi trả lời");
    await hostPage
      .getByText("Hãy tạo test case dữ liệu bị thiếu và vượt kích thước.", {
        exact: true,
      })
      .waitFor();
    await clickSave(hostPage, "Đóng trao đổi");
    await hostPage.getByRole("button", { name: "Mở lại trao đổi" }).waitFor();
  });
  await check("Content editor creates and publishes a lesson", async () => {
    await navigate(hostPage, "/quan-tri/hoc-lieu/embedded/new");
    await hostPage.getByLabel("Mã bài", { exact: true }).fill("browser-lesson");
    await hostPage
      .getByLabel("Tên bài học", { exact: true })
      .fill("Bài tạo từ trình duyệt");
    await hostPage
      .getByLabel("Tóm tắt", { exact: true })
      .fill("Nội dung tạo và xuất bản trong kiểm thử.");
    await hostPage
      .getByLabel("Trạng thái", { exact: true })
      .selectOption("published");
    await hostPage.getByLabel("Bài học mở").check();
    await hostPage.getByRole("button", { name: "Lưu bài học" }).click();
    await hostPage.waitForURL("**/quan-tri/hoc-lieu");
    await navigate(page, "/lop-hoc/embedded/browser-lesson");
    await page
      .getByRole("heading", { name: "Bài tạo từ trình duyệt", exact: true })
      .waitFor();
  });
  await check("Resource upload attaches to edited lesson", async () => {
    await navigate(hostPage, "/quan-tri/hoc-lieu/embedded/browser-lesson");
    await hostPage
      .locator("[data-upload]")
      .setInputFiles({
        name: "lesson-notes.txt",
        mimeType: "text/plain",
        buffer: Buffer.from("Lesson resource uploaded from browser"),
      });
    await hostPage.locator(".upload-item").waitFor();
    await clickSave(hostPage, "Gắn tệp vào bài học");
    await hostPage
      .getByRole("link", { name: "lesson-notes.txt", exact: true })
      .waitFor();
  });
  const id = (await client.call("GET", "/api/admin/accounts?q=browser-student"))
    .data.items[0].id;
  await check(
    "Destructive confirmation can be cancelled with Escape",
    async () => {
      await navigate(hostPage, "/quan-tri/tai-khoan/" + id);
      await hostPage
        .getByText("Khóa tài khoản", { exact: true })
        .first()
        .click();
      const form = hostPage.locator(
        'form[data-api="/api/admin/users/' + id + '"]',
      );
      await form
        .getByLabel("Lý do", { exact: true })
        .fill("Kiểm tra hộp thoại xác nhận.");
      await form
        .getByRole("button", { name: "Khóa tài khoản", exact: true })
        .click();
      await hostPage.locator("#confirmation").waitFor({ state: "visible" });
      await hostPage.keyboard.press("Escape");
      assert.equal(
        (await client.call("GET", "/api/admin/accounts/" + id)).data.user
          .blocked,
        false,
      );
    },
  );
  await check("Mobile navigation and dark mode persist", async () => {
    await page.setViewportSize({ width: 390, height: 844 });
    await navigate(page, "/");
    await page.getByRole("button", { name: "Mở điều hướng" }).click();
    await page.locator(".sidebar.open").waitFor();
    await page
      .locator(".sidebar")
      .getByRole("link", { name: "Thiết kế mạch KiCad" })
      .click();
    await page.waitForURL("**/lop-hoc/kicad");
    await page.getByRole("button", { name: "Đổi giao diện sáng tối" }).click();
    await page.reload();
    assert.equal(await page.locator("html").getAttribute("data-theme"), "dark");
    await page.screenshot({
      path: root + "/test-results/course-mobile-dark.png",
      fullPage: true,
    });
    await page.getByRole("button", { name: "Đổi giao diện sáng tối" }).click();
  });
  const studentPaths = [
    "/",
    "/lop-hoc/embedded",
    "/lop-hoc/embedded/" + lesson.id,
    "/thu-vien/kicad",
    "/tai-khoan",
    "/tham-gia-lab",
    "/thuc-hanh/embedded",
    "/thuc-hanh/embedded/" + exercise.id,
    submissionURL,
    "/trao-doi/embedded",
    threadURL,
    "/cuoc-thi",
  ];
  for (const path of studentPaths)
    await check("Mobile layout and labels: " + path, async () => {
      await navigate(page, path);
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth > innerWidth + 1,
      );
      assert.equal(overflow, false, path + " overflows");
      const unlabeled = await page.evaluate(
        () =>
          [
            ...document.querySelectorAll(
              "input:not([type=hidden]),select,textarea",
            ),
          ].filter((el) => !el.labels?.length && !el.getAttribute("aria-label"))
            .length,
      );
      assert.equal(unlabeled, 0, path + " lacks input labels");
    });
  await hostPage.setViewportSize({ width: 390, height: 844 });
  for (const path of [
    "/quan-tri",
    "/quan-tri/thanh-vien?status=approved",
    "/quan-tri/tai-khoan",
    "/quan-tri/tai-khoan/" + id,
    "/quan-tri/hoc-lieu",
    "/quan-tri/hoc-lieu/embedded/browser-lesson",
    "/quan-tri/thuc-hanh/embedded",
    "/quan-tri/nhat-ky",
    "/quan-tri/cuoc-thi",
  ])
    await check("Admin mobile layout: " + path, async () => {
      await navigate(hostPage, path);
      assert.equal(
        await hostPage.evaluate(
          () => document.documentElement.scrollWidth > innerWidth + 1,
        ),
        false,
        path + " overflows",
      );
    });
  await hostPage.screenshot({
    path: root + "/test-results/admin-mobile.png",
    fullPage: true,
  });
  await check("Keyboard can reach skip link", async () => {
    await navigate(page, "/");
    await page.keyboard.press("Tab");
    assert.equal(
      await page.locator(":focus").textContent(),
      "Đến nội dung chính",
    );
  });
  await check("No JavaScript exceptions or CSP violations", () =>
    assert.deepEqual(errors, []),
  );
  await fs.writeFile(
    root + "/test-results/browser.json",
    JSON.stringify({ passed: checks.length, checks, errors }, null, 2),
  );
  console.log("Browser checks passed: " + checks.length);
} catch (error) {
  await page.screenshot({
    path: root + "/test-results/failure-student.png",
    fullPage: true,
  });
  await hostPage.screenshot({
    path: root + "/test-results/failure-host.png",
    fullPage: true,
  });
  throw error;
} finally {
  await browser.close();
  await e.stop();
}
