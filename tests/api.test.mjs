import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { startEnvironment, api, createLearner, root } from "./environment.mjs";
import { evidence } from "./image.mjs";
test(
  "EP Lab Rust — full API workflows and security",
  { timeout: 180000 },
  async (t) => {
    const e = await startEnvironment();
    const guest = api(e.origin),
      host = api(e.origin);
    try {
      await t.test("Health, security headers and 100 lessons", async () => {
        const r = await guest.call("GET", "/api/health");
        assert.equal(r.data.ok, true);
        assert.equal(
          r.response.headers.get("x-content-type-options"),
          "nosniff",
        );
        assert.match(
          r.response.headers.get("content-security-policy"),
          /script-src 'self'/,
        );
        const catalog = (await guest.call("GET", "/api/classroom")).data;
        assert.equal(
          catalog.reduce((n, c) => n + c.lessonCount, 0),
          100,
        );
        assert.equal(catalog.length, 3);
      });
      await t.test("Guest cannot access administration", async () => {
        for (const path of [
          "/api/admin/overview",
          "/api/admin/accounts",
          "/api/admin/lab-memberships",
          "/api/admin/audit",
          "/api/admin/lessons",
        ])
          await guest.call("GET", path, undefined, 401);
      });
      await t.test("Origin checks cover unauthenticated writes", async () => {
        await guest.call("POST", "/api/auth/register", {}, 403, {
          origin: "https://evil.example",
        });
      });
      await t.test("Invalid registration rejected", async () => {
        await guest.call(
          "POST",
          "/api/auth/register",
          { name: "Ab", email: "bad", phone: "abc", password: "short" },
          400,
        );
      });
      await host.call("POST", "/api/auth/login", {
        email: e.adminEmail,
        password: e.password,
      });
      const hostID = (await host.call("GET", "/api/auth/me")).data.user.id;
      const learner = await createLearner(
        e,
        "Học viên <script>alert(1)</script>",
        "a",
      );
      const learnerB = await createLearner(e, "Học viên B", "b");
      const user = learner.user,
        other = learnerB.user;
      await t.test("Verification tokens are one-use", () =>
        guest.call("POST", "/api/auth/verify", { token: learner.token }, 400),
      );
      await t.test("Session cookie is HttpOnly and SameSite", async () => {
        const r = await guest.call("POST", "/api/auth/login", {
          email: learner.email,
          password: e.password,
        });
        assert.match(r.response.headers.get("set-cookie"), /HttpOnly/);
        assert.match(r.response.headers.get("set-cookie"), /SameSite=Lax/);
      });
      await t.test("User cannot promote their role", () =>
        user.call(
          "PATCH",
          "/api/auth/profile",
          { name: "User", phone: "0900000001", role: "admin" },
          422,
        ),
      );
      await t.test("Student cannot access host account APIs", () =>
        user.call("GET", "/api/admin/accounts", undefined, 403),
      );
      await t.test("Missing CSRF and mismatched Origin rejected", async () => {
        await user.call(
          "PATCH",
          "/api/auth/profile",
          { name: "User", phone: "0900000001" },
          403,
          { "x-csrf-token": "" },
        );
        await user.call(
          "PATCH",
          "/api/auth/profile",
          { name: "User", phone: "0900000001" },
          403,
          { origin: "https://attacker.invalid" },
        );
      });
      const catalog = JSON.parse(
        await fs.readFile(root + "/content/courses.json", "utf8"),
      );
      const preview = catalog.embedded.lessons.find((l) => l.preview);
      const locked = catalog.embedded.lessons.find((l) => !l.preview);
      await t.test(
        "Preview has no quiz answer or tutor answer leakage",
        async () => {
          const lesson = (
            await api(e.origin).call(
              "GET",
              "/api/classroom/embedded/lessons/" + preview.id,
            )
          ).data.lesson;
          assert.equal(lesson.quiz.answer, undefined);
          assert.equal(lesson.quiz.explanation, undefined);
          assert.ok(
            (lesson.questions || []).every((q) => typeof q === "string"),
          );
        },
      );
      await t.test(
        "Unapproved student denied full lesson and handbook",
        async () => {
          await user.call(
            "GET",
            "/api/classroom/embedded/lessons/" + locked.id,
            undefined,
            403,
          );
          await user.call(
            "GET",
            "/api/classroom/embedded/handbook",
            undefined,
            403,
          );
        },
      );
      const request = {
        tracks: ["embedded", "kicad"],
        message: "Tôi muốn học thiết kế mạch và lập trình nhúng.",
      };
      await t.test("Membership requires valid tracks and meaningful goal", () =>
        user.call(
          "POST",
          "/api/lab/membership",
          { tracks: ["invalid"], message: "abc" },
          400,
        ),
      );
      await t.test(
        "Membership requests are idempotent while pending",
        async () => {
          const a = (await user.call("POST", "/api/lab/membership", request))
            .data;
          const b = (await user.call("POST", "/api/lab/membership", request))
            .data;
          assert.equal(a.duplicate, false);
          assert.equal(b.duplicate, true);
        },
      );
      let membership = (await user.call("GET", "/api/lab/membership")).data
        .request;
      await t.test("Approval requires no typed phrase or note", () =>
        host.call("PATCH", "/api/admin/lab-memberships/" + learner.id, {
          status: "approved",
          revision: membership.revision,
        }),
      );
      await t.test("Stale approval cannot overwrite new decision", () =>
        host.call(
          "PATCH",
          "/api/admin/lab-memberships/" + learner.id,
          {
            status: "rejected",
            revision: membership.revision,
            note: "Không còn phù hợp",
          },
          409,
        ),
      );
      await t.test("Approval grants all three tracks immediately", async () => {
        for (const track of ["embedded", "kicad", "solidworks"]) {
          const l = catalog[track].lessons.find((l) => !l.preview);
          await user.call(
            "GET",
            "/api/classroom/" + track + "/lessons/" + l.id,
          );
        }
        assert.equal(
          (await user.call("GET", "/api/auth/me")).data.user.active,
          true,
        );
      });
      await other.call("POST", "/api/lab/membership", request);
      await host.call("PATCH", "/api/admin/lab-memberships/" + learnerB.id, {
        status: "approved",
        revision: 1,
      });
      const lessonURL = "/api/classroom/embedded/lessons/" + preview.id;
      await t.test("Cannot mark completion before quiz and all steps", () =>
        user.call(
          "PUT",
          lessonURL + "/progress",
          { completed: true, checklist: [], note: "" },
          400,
        ),
      );
      await t.test("Incorrect quiz and out-of-range answer", async () => {
        const wrong = (preview.quiz.answer + 1) % preview.quiz.options.length;
        assert.equal(
          (await user.call("POST", lessonURL + "/quiz", { answer: wrong })).data
            .correct,
          false,
        );
        await user.call("POST", lessonURL + "/quiz", { answer: 999 }, 400);
      });
      await t.test("Quiz correctness comes from server", async () => {
        assert.equal(
          (
            await user.call("POST", lessonURL + "/quiz", {
              answer: preview.quiz.answer,
            })
          ).data.correct,
          true,
        );
      });
      await t.test("Duplicate/out-of-range checklist is rejected", () =>
        user.call(
          "PUT",
          lessonURL + "/progress",
          { checklist: [0, 0], note: "" },
          400,
        ),
      );
      await t.test("Completion and notes persist", async () => {
        await user.call("PUT", lessonURL + "/progress", {
          scene: 1,
          note: "Ghi chú riêng học viên A",
          checklist: preview.steps.map((_, i) => i),
          completed: true,
        });
        const d = (await user.call("GET", lessonURL)).data;
        assert.equal(d.progress.completed, true);
        assert.equal(d.progress.note, "Ghi chú riêng học viên A");
      });
      await t.test("Progress remains private between accounts", async () => {
        const d = (await other.call("GET", lessonURL)).data;
        assert.equal(d.progress, null);
        assert.equal(d.questions.length, 0);
      });
      await t.test("Tutor cites readable handbook sources", async () => {
        const d = (
          await user.call("POST", lessonURL + "/questions", {
            question: preview.title,
          })
        ).data;
        assert.ok(JSON.stringify(d).includes("references"));
        assert.ok(JSON.stringify(d).includes("/lop-hoc/embedded/"));
      });
      await t.test(
        "Handbook PDF works and host handbook is protected",
        async () => {
          const r = await user.call("GET", "/api/classroom/embedded/handbook");
          assert.ok(r.raw.startsWith("%PDF-"));
          await user.call(
            "GET",
            "/api/admin/handbooks/embedded",
            undefined,
            403,
          );
          assert.ok(
            (
              await host.call("GET", "/api/admin/handbooks/embedded")
            ).raw.startsWith("%PDF-"),
          );
        },
      );
      await t.test(
        "Search, profile and audit do not leak password hashes",
        async () => {
          const r = (
            await host.call(
              "GET",
              "/api/admin/accounts?q=learner-a&state=active",
            )
          ).data;
          assert.equal(r.items.length, 1);
          assert.equal(r.items[0].password_hash, undefined);
          const d = (
            await host.call("GET", "/api/admin/accounts/" + learner.id)
          ).data;
          assert.equal(d.user.password_hash, undefined);
          assert.ok(d.history.some((x) => x.action === "lab.approved"));
        },
      );
      await t.test("Host notes are saved", async () => {
        await host.call(
          "POST",
          "/api/admin/accounts/" + learner.id + "/notes",
          { note: "Đã trao đổi mục tiêu học tập" },
        );
        assert.ok(
          (
            await host.call("GET", "/api/admin/accounts/" + learner.id)
          ).data.history.some((x) => x.action === "user.note"),
        );
      });
      await t.test(
        "Administrator cannot be blocked or remotely logged out",
        async () => {
          await host.call(
            "PATCH",
            "/api/admin/users/" + hostID,
            { blocked: true, reason: "Test protection" },
            403,
          );
          await host.call(
            "POST",
            "/api/admin/accounts/" + hostID + "/revoke-sessions",
            { reason: "Test protection" },
            403,
          );
        },
      );
      const newLesson = {
        ...preview,
        id: "test-custom",
        title: "Bài kiểm thử Rust",
        preview: false,
      };
      const lessonEdit = {
        track: "embedded",
        id: newLesson.id,
        status: "draft",
        body: newLesson,
      };
      await t.test("Create a draft lesson without exposing it", async () => {
        await host.call("POST", "/api/admin/lessons", lessonEdit);
        await user.call(
          "GET",
          "/api/classroom/embedded/lessons/test-custom",
          undefined,
          404,
        );
        assert.equal(
          (await user.call("GET", "/api/classroom")).data.reduce(
            (n, c) => n + c.lessonCount,
            0,
          ),
          100,
        );
      });
      await t.test("Publish and count dynamic lessons", async () => {
        await host.call("PUT", "/api/admin/lessons/embedded/test-custom", {
          ...lessonEdit,
          status: "published",
          revision: 1,
        });
        await user.call("GET", "/api/classroom/embedded/lessons/test-custom");
        assert.equal(
          (await user.call("GET", "/api/classroom")).data.reduce(
            (n, c) => n + c.lessonCount,
            0,
          ),
          101,
        );
      });
      await t.test("Concurrent stale edit is rejected", () =>
        host.call(
          "PUT",
          "/api/admin/lessons/embedded/test-custom",
          { ...lessonEdit, revision: 1 },
          409,
        ),
      );
      await t.test(
        "Saved tutor answers respect later source visibility",
        async () => {
          const answer = (
            await user.call("POST", lessonURL + "/questions", {
              question: "Rust",
            })
          ).data;
          assert.equal(answer.references[0].id, "test-custom");
          await host.call("PUT", "/api/admin/lessons/embedded/test-custom", {
            ...lessonEdit,
            status: "draft",
            revision: 2,
          });
          assert.ok(
            !(await user.call("GET", lessonURL)).data.questions.some(
              (q) => q.question === "Rust",
            ),
          );
          await host.call("PUT", "/api/admin/lessons/embedded/test-custom", {
            ...lessonEdit,
            status: "published",
            revision: 3,
          });
          assert.ok(
            (await user.call("GET", lessonURL)).data.questions.some(
              (q) => q.question === "Rust",
            ),
          );
        },
      );
      await t.test("Invalid lesson structure cannot crash the reader", () =>
        host.call(
          "POST",
          "/api/admin/lessons",
          {
            ...lessonEdit,
            id: "bad-structure",
            body: {
              ...newLesson,
              id: "bad-structure",
              concepts: [["Incomplete"]],
            },
          },
          400,
        ),
      );
      async function upload(client, filename, bytes) {
        const form = new FormData();
        form.append("file", new Blob([bytes]), filename);
        return (await client.call("POST", "/api/uploads", form)).data;
      }
      await t.test("Reject executable, SVG and false file type", async () => {
        for (const [name, data] of [
          ["bad.exe", "MZ"],
          ["bad.svg", "<svg/>"],
          ["bad.pdf", "not-pdf"],
          ["bad.c", "MZbinary"],
        ]) {
          const form = new FormData();
          form.append("file", new Blob([data]), name);
          await user.call("POST", "/api/uploads", form, 400);
        }
      });
      let image;
      await t.test(
        "Real multipart image upload decodes and normalizes",
        async () => {
          image = await upload(user, "evidence.png", evidence());
          assert.equal(image.contentType, "image/png");
          assert.ok(image.size > 1000);
        },
      );
      await t.test(
        "Other learner cannot download or delete private upload",
        async () => {
          await other.call("GET", "/api/files/" + image.id, undefined, 404);
          await other.call("DELETE", "/api/files/" + image.id, undefined, 404);
        },
      );
      const resource = await upload(
        host,
        "instructions.txt",
        "Private resource example",
      );
      await t.test(
        "Attach resource to lesson with permission checks",
        async () => {
          await host.call(
            "POST",
            "/api/admin/lessons/embedded/test-custom/resources",
            { fileId: resource.id },
          );
          const d = (
            await user.call(
              "GET",
              "/api/classroom/embedded/lessons/test-custom",
            )
          ).data;
          assert.equal(d.resources[0].id, resource.id);
          await user.call("GET", "/api/files/" + resource.id);
        },
      );
      await t.test("Referenced resource blocks deleting its lesson", () =>
        host.call(
          "DELETE",
          "/api/admin/lessons/embedded/test-custom",
          undefined,
          409,
        ),
      );
      const exercises = (
        await user.call("GET", "/api/practice/embedded/exercises")
      ).data;
      const exercise = (
        await user.call(
          "GET",
          "/api/practice/embedded/exercises/" + exercises[0].id,
        )
      ).data;
      const assignment = (
        await host.call("POST", "/api/admin/practice/embedded/assignments", {
          exerciseId: exercise.id,
          studentId: learner.id,
          instructions: "Làm đầy đủ các bước",
          dueAt: null,
        })
      ).data;
      await t.test(
        "Individual assignment is private to recipient",
        async () => {
          assert.ok(
            (
              await user.call("GET", "/api/practice/embedded/assignments")
            ).data.some((a) => a.id === assignment.id),
          );
          assert.ok(
            !(
              await other.call("GET", "/api/practice/embedded/assignments")
            ).data.some((a) => a.id === assignment.id),
          );
        },
      );
      const submission = {
        exerciseId: exercise.id,
        assignmentId: assignment.id,
        notes: "Đã chạy thử chương trình và ghi lại kết quả.",
        selfChecks: [],
        fileIds: [image.id],
      };
      let submitted;
      await t.test("Cannot submit another user's file", () =>
        other.call(
          "POST",
          "/api/practice/embedded/submissions",
          { ...submission, assignmentId: null },
          400,
        ),
      );
      await t.test(
        "Submission commits evidence and rubric snapshot",
        async () => {
          submitted = (
            await user.call(
              "POST",
              "/api/practice/embedded/submissions",
              submission,
            )
          ).data;
          const d = (
            await user.call("GET", "/api/practice/submissions/" + submitted.id)
          ).data;
          assert.equal(d.status, "pending_manual");
          assert.equal(d.files[0].id, image.id);
          assert.equal(d.rubric.title, exercise.title);
        },
      );
      await t.test("Retry cannot duplicate a submission", async () => {
        const retry = (
          await user.call(
            "POST",
            "/api/practice/embedded/submissions",
            submission,
          )
        ).data;
        assert.equal(retry.id, submitted.id);
        assert.equal(retry.duplicate, true);
      });
      await t.test("Other learner cannot view a submission", () =>
        other.call(
          "GET",
          "/api/practice/submissions/" + submitted.id,
          undefined,
          404,
        ),
      );
      await t.test("Submitted evidence cannot be erased", () =>
        user.call("DELETE", "/api/files/" + image.id, undefined, 409),
      );
      const review = {
        revision: 1,
        summary: "Đạt yêu cầu theo bằng chứng đã nộp.",
        criteria: exercise.criteria.map((c) => ({
          id: c.id,
          status: "met",
          evidence: "Quan sát đầy đủ chi tiết trong ảnh.",
          action: "",
        })),
      };
      await t.test("Student cannot grade their own work", () =>
        user.call(
          "POST",
          "/api/admin/practice/submissions/" + submitted.id + "/review",
          review,
          403,
        ),
      );
      await t.test("Incomplete rubric review rejected", () =>
        host.call(
          "POST",
          "/api/admin/practice/submissions/" + submitted.id + "/review",
          { ...review, criteria: review.criteria.slice(1) },
          400,
        ),
      );
      await t.test(
        "Server computes score and stores review history",
        async () => {
          const r = (
            await host.call(
              "POST",
              "/api/admin/practice/submissions/" + submitted.id + "/review",
              review,
            )
          ).data;
          assert.equal(r.result.score, 100);
          assert.equal(r.result.status, "passed");
          assert.equal(
            (
              await user.call(
                "GET",
                "/api/practice/submissions/" + submitted.id,
              )
            ).data.reviews.length,
            1,
          );
        },
      );
      await t.test("Stale grading cannot replace newer review", () =>
        host.call(
          "POST",
          "/api/admin/practice/submissions/" + submitted.id + "/review",
          review,
          409,
        ),
      );
      await t.test(
        "Closing assignment prevents later submissions",
        async () => {
          await host.call(
            "PATCH",
            "/api/admin/practice/assignments/" + assignment.id,
            { closed: true },
          );
          const f = await upload(user, "new-evidence.png", evidence());
          await user.call(
            "POST",
            "/api/practice/embedded/submissions",
            {
              ...submission,
              notes: "Bài làm bổ sung sau khi đóng nhận.",
              fileIds: [f.id],
            },
            403,
          );
          await user.call("DELETE", "/api/files/" + f.id);
        },
      );
      const conversation = (
        await user.call("POST", "/api/conversations/embedded", {
          subject: "Câu hỏi về firmware",
          body: "Em cần kiểm tra lỗi tràn bộ đệm.",
        })
      ).data;
      await t.test("Conversations remain private", async () => {
        await other.call(
          "GET",
          "/api/conversation/" + conversation.id,
          undefined,
          404,
        );
        assert.equal(
          (await other.call("GET", "/api/conversations/embedded")).data.length,
          0,
        );
      });
      const reply = {
        body: "Hãy kiểm tra giới hạn của chỉ số mảng.",
        requestId: randomUUID(),
      };
      await t.test("Message retry is idempotent", async () => {
        await host.call(
          "POST",
          "/api/conversation/" + conversation.id + "/messages",
          reply,
        );
        assert.equal(
          (
            await host.call(
              "POST",
              "/api/conversation/" + conversation.id + "/messages",
              reply,
            )
          ).data.duplicate,
          true,
        );
        assert.equal(
          (await user.call("GET", "/api/conversation/" + conversation.id)).data
            .messages.length,
          2,
        );
      });
      await t.test("Message request ID cannot be reused for changed text", () =>
        host.call(
          "POST",
          "/api/conversation/" + conversation.id + "/messages",
          { ...reply, body: "Nội dung khác" },
          409,
        ),
      );
      await t.test("Closed conversation rejects new messages", async () => {
        const d = (
          await user.call("GET", "/api/conversation/" + conversation.id)
        ).data;
        await user.call("PATCH", "/api/conversation/" + conversation.id, {
          closed: true,
          revision: d.revision,
        });
        await host.call(
          "POST",
          "/api/conversation/" + conversation.id + "/messages",
          { ...reply, requestId: randomUUID() },
          409,
        );
      });
      await t.test("Competition sources reject script URLs", () =>
        host.call(
          "POST",
          "/api/admin/competitions",
          {
            id: "test",
            title: "Test competition",
            organizer: "Test",
            summary: "Test competition summary",
            source_url: "javascript:alert(1)",
            deadline: "",
          },
          400,
        ),
      );
      await t.test("Competition creation and hiding work", async () => {
        await host.call("POST", "/api/admin/competitions", {
          id: "test",
          title: "Test competition",
          organizer: "Test",
          summary: "Test competition summary",
          source_url: "https://example.com/contest",
          deadline: "",
        });
        assert.equal(
          (await user.call("GET", "/api/competitions")).data.length,
          1,
        );
        await host.call("PATCH", "/api/admin/competitions/test", {
          hidden: true,
          revision: 1,
        });
        assert.equal(
          (await user.call("GET", "/api/competitions")).data.length,
          0,
        );
      });
      await t.test("HTML escapes stored user-controlled content", async () => {
        const r = await user.call("GET", "/tai-khoan");
        assert.ok(r.raw.includes("&lt;script&gt;"));
        assert.ok(!r.raw.includes("<script>alert(1)</script>"));
      });
      await t.test("Every major admin page renders", async () => {
        for (const path of [
          "/quan-tri",
          "/quan-tri/thanh-vien",
          "/quan-tri/tai-khoan",
          "/quan-tri/tai-khoan/" + learner.id,
          "/quan-tri/hoc-lieu",
          "/quan-tri/hoc-lieu/embedded/test-custom",
          "/quan-tri/nhat-ky",
          "/quan-tri/cuoc-thi",
          "/quan-tri/thuc-hanh/embedded",
        ]) {
          const r = await host.call("GET", path);
          assert.ok(r.raw.includes("<html"));
        }
      });
      await t.test("Every learner page renders", async () => {
        for (const path of [
          "/",
          "/lop-hoc/embedded",
          "/lop-hoc/embedded/" + preview.id,
          "/thu-vien/embedded",
          "/tham-gia-lab",
          "/tai-khoan",
          "/thuc-hanh/embedded",
          "/thuc-hanh/embedded/" + exercise.id,
          "/bai-nop/" + submitted.id,
          "/trao-doi/embedded",
          "/trao-doi/chu-de/" + conversation.id,
          "/cuoc-thi",
        ]) {
          assert.ok((await user.call("GET", path)).raw.includes("<html"));
        }
      });
      await t.test(
        "Course draft removes full access and course listing",
        async () => {
          const c = (await host.call("GET", "/api/admin/courses")).data.find(
            (c) => c.category === "embedded",
          );
          await host.call("PUT", "/api/admin/courses/embedded", {
            title: c.title,
            description: c.description,
            status: "draft",
            revision: c.revision,
          });
          await user.call("GET", lessonURL, undefined, 404);
          await user.call(
            "GET",
            "/api/practice/embedded/exercises/" + exercise.id,
            undefined,
            403,
          );
          assert.equal(
            (await user.call("GET", "/api/classroom")).data.length,
            2,
          );
          await host.call("PUT", "/api/admin/courses/embedded", {
            title: c.title,
            description: c.description,
            status: "published",
            revision: c.revision + 1,
          });
        },
      );
      await t.test(
        "Resource and lesson deletion persist across seed",
        async () => {
          await host.call("DELETE", "/api/files/" + resource.id);
          await host.call("DELETE", "/api/admin/lessons/embedded/test-custom");
          await e.run(["seed"]);
          await host.call(
            "GET",
            "/api/classroom/embedded/lessons/test-custom",
            undefined,
            404,
          );
        },
      );
      await t.test(
        "Deleting seeded content survives another seed",
        async () => {
          const id = catalog.kicad.lessons.at(-1).id;
          await host.call("DELETE", "/api/admin/lessons/kicad/" + id);
          await e.run(["seed"]);
          await host.call(
            "GET",
            "/api/classroom/kicad/lessons/" + id,
            undefined,
            404,
          );
        },
      );
      await t.test(
        "Concurrent membership decisions have exactly one winner",
        async () => {
          const race = await createLearner(e, "Race learner", "race");
          await race.user.call("POST", "/api/lab/membership", request);
          const calls = await Promise.all(
            ["approved", "rejected"].map((status) =>
              fetch(e.origin + "/api/admin/lab-memberships/" + race.id, {
                method: "PATCH",
                headers: {
                  origin: e.origin,
                  cookie: host.cookie,
                  "x-csrf-token": host.csrf,
                  "content-type": "application/json",
                },
                body: JSON.stringify({
                  status,
                  revision: 1,
                  note: "Concurrent review test",
                }),
              }),
            ),
          );
          assert.deepEqual(calls.map((r) => r.status).sort(), [200, 409]);
          assert.equal(
            (
              await e.client.query(
                "SELECT count(*)::int AS n FROM audit_logs WHERE target=$1 AND action IN('lab.approved','lab.rejected')",
                [race.id],
              )
            ).rows[0].n,
            1,
          );
        },
      );
      await t.test(
        "Revocation requires reason and is immediately enforced",
        async () => {
          membership = (await user.call("GET", "/api/lab/membership")).data
            .request;
          await host.call(
            "PATCH",
            "/api/admin/lab-memberships/" + learner.id,
            { status: "revoked", revision: membership.revision },
            400,
          );
          await host.call("PATCH", "/api/admin/lab-memberships/" + learner.id, {
            status: "revoked",
            revision: membership.revision,
            note: "Kết thúc đợt học thử nghiệm.",
          });
          await user.call(
            "GET",
            "/api/classroom/embedded/lessons/" + locked.id,
            undefined,
            403,
          );
          await user.call("GET", "/api/files/" + image.id, undefined, 403);
          await user.call(
            "GET",
            "/api/conversation/" + conversation.id,
            undefined,
            403,
          );
        },
      );
      await t.test("Blocked user sessions become unusable", async () => {
        await host.call("PATCH", "/api/admin/users/" + learnerB.id, {
          blocked: true,
          reason: "Kiểm thử khóa tài khoản.",
        });
        assert.equal((await other.call("GET", "/api/auth/me")).data.user, null);
        await other.call("GET", "/api/auth/sessions", undefined, 401);
      });
      await t.test("Unblocking does not restore old sessions", async () => {
        await host.call("PATCH", "/api/admin/users/" + learnerB.id, {
          blocked: false,
        });
        assert.equal((await other.call("GET", "/api/auth/me")).data.user, null);
        await other.call("POST", "/api/auth/login", {
          email: learnerB.email,
          password: e.password,
        });
      });
      await t.test(
        "Host invitation creates no usable password until accepted",
        async () => {
          const email = "invited@example.test";
          const d = (
            await host.call("POST", "/api/admin/accounts", {
              name: "Người được mời",
              email,
              phone: "0900000002",
            })
          ).data;
          assert.equal(d.invitationSent, true);
          const token = await e.mailToken(email);
          await api(e.origin).call(
            "POST",
            "/api/auth/accept-invitation",
            { token, password: e.password, acceptedTerms: false },
            400,
          );
          await api(e.origin).call("POST", "/api/auth/accept-invitation", {
            token,
            password: e.password,
            acceptedTerms: true,
          });
          await api(e.origin).call(
            "POST",
            "/api/auth/accept-invitation",
            { token, password: e.password, acceptedTerms: true },
            400,
          );
          await api(e.origin).call("POST", "/api/auth/login", {
            email,
            password: e.password,
          });
        },
      );
      await t.test(
        "Password recovery revokes sessions and token is one-use",
        async () => {
          await api(e.origin).call("POST", "/api/auth/forgot", {
            email: learnerB.email,
          });
          const token = await e.mailToken(learnerB.email);
          await api(e.origin).call("POST", "/api/auth/reset", {
            token,
            password: e.password + "x",
          });
          assert.equal(
            (await other.call("GET", "/api/auth/me")).data.user,
            null,
          );
          await api(e.origin).call(
            "POST",
            "/api/auth/reset",
            { token, password: e.password + "x" },
            400,
          );
        },
      );
      await t.test(
        "Persistent credential rate limit rejects brute force",
        async () => {
          const bad = api(e.origin);
          for (let i = 0; i < 10; i++)
            await bad.call(
              "POST",
              "/api/auth/login",
              {
                email: "missing@example.test",
                password: "not-the-real-password",
              },
              401,
            );
          await bad.call(
            "POST",
            "/api/auth/login",
            {
              email: "missing@example.test",
              password: "not-the-real-password",
            },
            429,
          );
        },
      );
      await t.test(
        "Audit includes review, revocation and blocking",
        async () => {
          const rows = (
            await e.client.query("SELECT DISTINCT action FROM audit_logs")
          ).rows.map((x) => x.action);
          for (const action of [
            "submission.reviewed",
            "lab.revoked",
            "user.blocked",
            "lesson.deleted",
            "user.password_reset",
          ])
            assert.ok(rows.includes(action));
        },
      );
      await t.test("No server panic or internal error occurred", () => {
        assert.ok(
          !/panicked|database operation failed/.test(e.logs()),
          e.logs(),
        );
      });
    } finally {
      await e.stop();
    }
  },
);
