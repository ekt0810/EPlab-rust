(() => {
  "use strict";
  const csrf = () =>
    document.querySelector('meta[name="csrf-token"]')?.content || "";
  const toast = document.querySelector("#toast");
  try {
    const theme = localStorage.getItem("ep-theme");
    if (theme) document.documentElement.dataset.theme = theme;
    const flash = sessionStorage.getItem("ep-message");
    if (flash && toast) {
      toast.textContent = flash;
      toast.hidden = false;
      sessionStorage.removeItem("ep-message");
    }
  } catch {}
  document
    .querySelector("[data-theme-toggle]")
    ?.addEventListener("click", () => {
      const theme =
        document.documentElement.dataset.theme === "dark" ? "light" : "dark";
      document.documentElement.dataset.theme = theme;
      try {
        localStorage.setItem("ep-theme", theme);
      } catch {}
    });
  const menu = document.querySelector("[data-menu]");
  menu?.addEventListener("click", () => {
    const open = document.querySelector("#sidebar").classList.toggle("open");
    menu.setAttribute("aria-expanded", String(open));
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      document.querySelector("#sidebar")?.classList.remove("open");
      menu?.setAttribute("aria-expanded", "false");
    }
  });
  document.addEventListener("click", (event) => {
    if (!event.target.closest("#sidebar,[data-menu]")) {
      document.querySelector("#sidebar")?.classList.remove("open");
      menu?.setAttribute("aria-expanded", "false");
    }
  });
  for (const form of document.querySelectorAll("[data-token-form]")) {
    form.elements.token.value =
      new URLSearchParams(location.hash.slice(1)).get("token") || "";
  }
  if (document.querySelector("[data-token-form]") && location.hash)
    history.replaceState(null, "", location.pathname + location.search);
  function result(form, message, error = false) {
    let el = form.querySelector(".form-result");
    if (!el) {
      el = document.createElement("div");
      el.className = "form-result";
      el.setAttribute("role", "status");
      form.append(el);
    }
    el.hidden = false;
    el.classList.toggle("error", error);
    el.textContent = message;
    return el;
  }
  async function request(url, method, body) {
    const headers = { "x-csrf-token": csrf() };
    const init = { method, headers, credentials: "same-origin" };
    if (body !== undefined) {
      if (body instanceof FormData) {
        init.body = body;
      } else {
        headers["content-type"] = "application/json";
        init.body = JSON.stringify(body);
      }
    }
    const response = await fetch(url, init);
    const data = await response
      .json()
      .catch(() => ({
        error: "Phản hồi không hợp lệ. Vui lòng tải lại trang.",
      }));
    if (!response.ok)
      throw new Error(
        data.error ||
          data.message ||
          "Không thể lưu. Kiểm tra dữ liệu và thử lại.",
      );
    return data;
  }
  function setPath(obj, path, value) {
    const parts = path.split(".");
    let dest = obj;
    for (let i = 0; i < parts.length - 1; i++) {
      const k = parts[i];
      if (["__proto__", "constructor", "prototype"].includes(k))
        throw new Error("Trường không hợp lệ.");
      dest = dest[k] ??= /^\d+$/.test(parts[i + 1]) ? [] : {};
    }
    dest[parts.at(-1)] = value;
  }
  function bodyOf(form) {
    const body = {};
    for (const el of form.elements) {
      if (
        !el.name ||
        el.disabled ||
        ["submit", "button", "file"].includes(el.type)
      )
        continue;
      if (el.type === "radio" && !el.checked) continue;
      const array = el.name.endsWith("[]");
      const name = array ? el.name.slice(0, -2) : el.name;
      if (array) body[name] ??= [];
      if (el.type === "checkbox" && array && !el.checked) continue;
      let value = el.type === "checkbox" && !array ? el.checked : el.value;
      if (el.dataset.type === "boolean") value = value === "true";
      else if (
        el.dataset.type === "number" ||
        el.type === "number" ||
        ["revision", "scene"].includes(name)
      )
        value = Number(value);
      else if (el.hasAttribute("data-json")) {
        try {
          value = JSON.parse(value);
        } catch {
          throw new Error(
            "Nội dung JSON chưa hợp lệ. Kiểm tra dấu phẩy, ngoặc và dấu nháy.",
          );
        }
      } else if (el.type === "datetime-local" && value)
        value = new Date(value).toISOString();
      else if (el.hasAttribute("data-null") && !value) value = null;
      if (array) body[name].push(value);
      else setPath(body, name, value);
    }
    if (form.dataset.kind === "lesson") {
      Object.assign(body.body, {
        id: body.id,
        title: body.title,
        summary: body.summary,
        module: body.module,
        minutes: body.minutes,
        preview: body.preview,
      });
      const lines = (value) =>
        String(value || "")
          .split(/\r?\n/)
          .map((x) => x.trim())
          .filter(Boolean);
      Object.assign(body.body, {
        objectives: lines(body.lessonObjectives),
        steps: lines(body.lessonSteps),
        mistakes: lines(body.lessonMistakes),
        sources: lines(body.lessonSources),
        deliverable: body.lessonDeliverable,
        concepts: String(body.lessonConcepts || "")
          .trim()
          .split(/\n\s*\n/)
          .filter(Boolean)
          .map((block) => {
            const [title, ...rest] = block.split(/\r?\n/);
            return [title.trim(), rest.join("\n").trim()];
          }),
        quiz: {
          question: body.quizQuestion,
          options: lines(body.quizOptions),
          answer: Number(body.quizAnswer) - 1,
          explanation: body.quizExplanation,
        },
      });
      for (const k of Object.keys(body))
        if (k.startsWith("lesson") || k.startsWith("quiz")) delete body[k];
      for (const k of ["title", "summary", "module", "minutes", "preview"])
        delete body[k];
    }
    if (form.dataset.kind === "resource") {
      if (body.fileIds?.length !== 1)
        throw new Error("Chọn đúng một tệp học liệu.");
      return { fileId: body.fileIds[0] };
    }
    return body;
  }
  function confirmAction(message, label) {
    return new Promise((resolve) => {
      const dialog = document.querySelector("#confirmation");
      dialog.querySelector("#confirm-description").textContent = message;
      dialog.querySelector("#confirm-button").textContent = label;
      dialog.returnValue = "cancel";
      dialog.addEventListener(
        "close",
        () => resolve(dialog.returnValue === "confirm"),
        { once: true },
      );
      dialog.showModal();
    });
  }
  for (const form of document.querySelectorAll("form[data-api]")) {
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      if (form.dataset.busy === "true" || form.dataset.uploading === "true")
        return;
      if (!form.reportValidity()) return;
      const button =
        event.submitter || form.querySelector('[type="submit"],button');
      try {
        const body = bodyOf(form);
        if (
          form.dataset.confirm &&
          !(await confirmAction(
            form.dataset.confirm,
            button?.textContent.trim() || "Xác nhận",
          ))
        )
          return;
        form.dataset.busy = "true";
        if (button) button.disabled = true;
        const data = await request(
          form.dataset.api,
          form.dataset.method || "POST",
          body,
        );
        if (form.dataset.kind === "quiz") {
          result(
            form,
            (data.correct ? "Đúng. " : "Chưa đúng. ") +
              (data.explanation || ""),
            !data.correct,
          );
          if (data.correct) {
            const el = document.querySelector(".quiz-state");
            if (el) el.textContent = "✓ Đã vượt qua tự kiểm";
          }
          return;
        }
        const message = data.message || "Đã lưu.";
        const redirect = data.redirect || form.dataset.redirect;
        if (redirect) {
          const target = new URL(redirect, location.origin);
          if (target.origin !== location.origin)
            throw new Error("Liên kết chuyển trang không hợp lệ.");
          try {
            sessionStorage.setItem("ep-message", message);
          } catch {}
          location.assign(target.href);
        } else if (data.reload || form.dataset.reload === "true") {
          try {
            sessionStorage.setItem("ep-message", message);
          } catch {}
          location.reload();
        } else result(form, message);
      } catch (error) {
        result(form, error.message, true);
      } finally {
        form.dataset.busy = "false";
        if (button) button.disabled = false;
      }
    });
  }
  for (const input of document.querySelectorAll("[data-upload]")) {
    input.addEventListener("change", async () => {
      const form = input.closest("form");
      if (form.dataset.uploading === "true") return;
      form.dataset.uploading = "true";
      const submit = form.querySelector('[type="submit"]');
      if (submit) submit.disabled = true;
      try {
        for (const file of input.files) {
          if (file.size > 10 * 1024 * 1024)
            throw new Error("Mỗi tệp không quá 10 MB.");
          if (form.querySelectorAll('[name="fileIds[]"]').length >= 8)
            throw new Error("Tối đa 8 tệp cho một bài nộp.");
          const body = new FormData();
          body.append("file", file);
          const data = await request("/api/uploads", "POST", body);
          const row = document.createElement("div");
          row.className = "upload-item";
          const name = document.createElement("span");
          name.textContent = data.name;
          const hidden = document.createElement("input");
          hidden.type = "hidden";
          hidden.name = "fileIds[]";
          hidden.value = data.id;
          const remove = document.createElement("button");
          remove.type = "button";
          remove.className = "button compact";
          remove.textContent = "Gỡ";
          remove.addEventListener("click", async () => {
            remove.disabled = true;
            try {
              await request("/api/files/" + data.id, "DELETE");
              row.remove();
            } catch (error) {
              result(form, error.message, true);
              remove.disabled = false;
            }
          });
          row.append(name, hidden, remove);
          form.querySelector(".uploads").append(row);
        }
        result(form, "Tệp đã tải lên. Bấm nút bên dưới để hoàn tất.");
      } catch (error) {
        result(form, error.message, true);
      } finally {
        input.value = "";
        form.dataset.uploading = "false";
        if (submit) submit.disabled = false;
      }
    });
  }
})();
