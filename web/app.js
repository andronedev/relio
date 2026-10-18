import { initI18n, languagePicker, locale, t, tStrong } from "/i18n.js";

const TOKEN_KEY = "relio.token";
const STATUS_KEYS = { reading: "statusReading", unread: "statusUnread", finished: "statusFinished" };
const KIND_KEYS = { highlight: "kindHighlight", note: "kindNote", bookmark: "kindBookmark", markup: "kindMarkup" };
// As the Clara Colour renders Nickel's indices (0 verified on device, the rest follow its picker order).
const HIGHLIGHT_COLORS = ["colorCyan", "colorPurple", "colorYellow", "colorGreen", "colorGray"];
const EXCERPT_CHARS = 80;

const state = { books: [], annotations: [], shelves: [], filter: "all", query: "" };
const $ = (id) => document.getElementById(id);
let currentToken = null;

function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value == null || value === false) continue;
    if (key.startsWith("on")) el.addEventListener(key.slice(2), value);
    else if (key === "style") el.style.cssText = value;
    else el.setAttribute(key, value === true ? "" : value);
  }
  el.append(...children.flat().filter((c) => c != null && c !== false));
  return el;
}

const storage = {
  get: (key) => { try { return localStorage.getItem(key); } catch { return null; } },
  set: (key, value) => { try { localStorage.setItem(key, value); } catch { /* private mode: lasts for this page */ } },
  remove: (key) => { try { localStorage.removeItem(key); } catch { /* nothing stored */ } },
};

// Formatting

const percent = (value) => new Intl.NumberFormat(locale(), { style: "percent", maximumFractionDigits: 0 }).format(value / 100);

function duration(seconds) {
  if (!seconds) return null;
  const totalMinutes = Math.max(1, Math.round(seconds / 60));
  const unit = (value, name) => new Intl.NumberFormat(locale(), { style: "unit", unit: name, unitDisplay: "short" }).format(value);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (!hours) return unit(minutes, "minute");
  return minutes ? `${unit(hours, "hour")} ${unit(minutes, "minute")}` : unit(hours, "hour");
}

function relativeDate(iso) {
  const time = iso ? Date.parse(iso) : NaN;
  if (Number.isNaN(time)) return null;
  const days = Math.round((time - Date.now()) / 86_400_000);
  if (Math.abs(days) < 30) return new Intl.RelativeTimeFormat(locale(), { numeric: "auto" }).format(days, "day");
  return new Date(time).toLocaleDateString(locale(), { day: "numeric", month: "long", year: "numeric" });
}

const excerpt = (text) => (text.length > EXCERPT_CHARS ? `${text.slice(0, EXCERPT_CHARS).trimEnd()}…` : text);

function coverColor(book) {
  let hash = 2166136261;
  for (const char of book.id) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619) >>> 0;
  return `var(--cv-${hash % 4})`;
}

const highlightColor = (index) => `var(--hl-${index >= 0 && index < HIGHLIGHT_COLORS.length ? index : 0})`;

// API

class Unauthorized extends Error {}

const authorized = (path) => `${path}${path.includes("?") ? "&" : "?"}token=${encodeURIComponent(currentToken)}`;

async function api(path, auth) {
  const response = await fetch(`/api${path}`, { headers: { Authorization: `Bearer ${auth}` } });
  if (response.status === 401) throw new Unauthorized();
  if (!response.ok) throw new Error(`${path}: ${response.status}`);
  return response.json();
}

function deviceName() {
  const ua = navigator.userAgent;
  const browser = [["Edg", "Edge"], ["Firefox", "Firefox"], ["Chrome", "Chrome"], ["Safari", "Safari"]]
    .find(([token]) => ua.includes(token))?.[1] ?? t("browser");
  const os = ["iPhone", "iPad", "Android", "Mac", "Windows", "Linux"].find((name) => ua.includes(name));
  return os ? t("deviceName", { browser, os }) : browser;
}

async function pair(code) {
  const response = await fetch("/api/pair", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code, name: deviceName() }),
  }).catch(() => null);
  if (response?.status === 403) throw new Error(t("wrongCode"));
  if (response?.status === 410) throw new Error(t("noActiveCode"));
  if (!response?.ok) throw new Error(t("unreachable"));
  return (await response.json()).token;
}

// Rendering

function cover(book, { full = false } = {}) {
  const image = h("img", {
    alt: "",
    loading: "lazy",
    decoding: "async",
    src: authorized(`/api/books/cover?id=${encodeURIComponent(book.id)}${full ? "&full=true" : ""}`),
  });
  const element = h("div", { class: "cover", style: `--cover: ${coverColor(book)}`, "aria-hidden": "true" },
    h("span", { class: "cover-title", dir: "auto", lang: book.language }, book.title),
    h("span", { class: "cover-author", dir: "auto" }, book.author ?? ""),
    image);
  image.addEventListener("load", () => element.classList.add("has-image"));
  image.addEventListener("error", () => image.remove());
  return element;
}

const progressBar = (value, title) =>
  h("div", {
    class: "progress",
    role: "progressbar",
    "aria-valuenow": value,
    "aria-valuemin": 0,
    "aria-valuemax": 100,
    "aria-valuetext": percent(value),
    "aria-label": t("progressLabel", { title }),
  }, h("span", { style: `inline-size: ${value}%` }));

function renderDevice(device, online) {
  const el = $("device");
  el.toggleAttribute("data-offline", !online);
  el.textContent = online ? device.model : t("deviceOffline");
}

function renderResume() {
  const book = state.books.find((b) => b.status === "reading");
  const section = $("resume");
  section.replaceChildren();
  if (!book) return;
  const remaining = duration(book.seconds_remaining);
  section.append(
    cover(book),
    h("div", { class: "resume-text" },
      h("p", { class: "kicker" }, t("lastRead", { when: relativeDate(book.last_read_at) ?? t("recently") })),
      h("h1", { id: "resume-title", dir: "auto", lang: book.language }, book.title),
      book.author && h("p", { class: "author", dir: "auto" }, book.author),
      progressBar(book.percent_read, book.title),
      h("div", { class: "progress-meta" },
        h("span", {}, percent(book.percent_read)),
        remaining && h("span", {}, t("timeLeft", { duration: remaining })))));
}

function bookCard(book) {
  const status = t(STATUS_KEYS[book.status]);
  const detail = book.status === "reading" ? percent(book.percent_read) : status;
  return h("li", {},
    h("button", { class: "book", type: "button", onclick: () => openBook(book), "aria-label": `${book.title}, ${status}` },
      cover(book),
      book.status === "reading" ? progressBar(book.percent_read, book.title) : h("div", { class: "progress-slot", "aria-hidden": "true" }),
      h("div", { class: "book-meta" },
        h("strong", { dir: "auto", lang: book.language }, book.title),
        h("span", {}, [book.author, detail].filter(Boolean).join(", ")))));
}

function renderBooks() {
  const query = state.query.toLocaleLowerCase(locale());
  const visible = state.books.filter((b) =>
    (state.filter === "all" || b.status === state.filter) &&
    (!query || `${b.title} ${b.author ?? ""}`.toLocaleLowerCase(locale()).includes(query)));
  $("books").replaceChildren(...(visible.length
    ? visible.map(bookCard)
    : [h("li", { class: "empty" }, t(state.books.length ? "noMatch" : "emptyLibrary"))]));
}

function annotationItem(annotation) {
  const isBookmark = annotation.kind === "bookmark";
  const position = annotation.chapter_progress != null
    ? t("throughBook", { percent: percent(annotation.chapter_progress * 100) })
    : null;
  return h("figure", { class: `hl${isBookmark ? " hl-bookmark" : ""}`, style: `--hl-color: ${highlightColor(annotation.color)}` },
    h("blockquote", {}, isBookmark ? t("kindBookmark") : h("mark", { dir: "auto" }, annotation.text ?? "")),
    annotation.note && h("p", { class: "note", dir: "auto" }, annotation.note),
    h("figcaption", {},
      !isBookmark && h("span", {}, t(KIND_KEYS[annotation.kind])),
      position && h("span", {}, position),
      h("time", { datetime: annotation.created_at }, relativeDate(annotation.created_at) ?? "")),
    annotationActions(annotation, isBookmark));
}

function annotationActions(annotation, isBookmark) {
  const actions = h("div", { class: "hl-actions" });
  if (!isBookmark) {
    actions.append(
      h("div", { class: "swatches", role: "group", "aria-label": t("highlightColor") },
        HIGHLIGHT_COLORS.map((key, index) =>
          h("button", {
            class: "swatch",
            type: "button",
            style: `--hl-color: ${highlightColor(index)}`,
            "aria-label": t(key),
            title: t(key),
            "aria-pressed": String(annotation.color === index),
            onclick: () => changeAnnotation(annotation, { color: index }),
          }))),
      h("button", { class: "link", type: "button", onclick: (event) => editNote(annotation, event.currentTarget.closest("figure")) },
        t(annotation.note ? "editNote" : "addNote")));
  }
  actions.append(h("button", { class: "link link-danger", type: "button", onclick: () => confirmAnnotationDelete(annotation) }, t("delete")));
  return actions;
}

function editNote(annotation, figure) {
  const field = h("textarea", { rows: 3, "aria-label": t("noteLabel"), maxlength: 10000 });
  field.value = annotation.note ?? "";
  const status = h("p", { class: "error", role: "alert" });
  const save = h("button", { class: "button", type: "button" }, t("save"));
  const cancel = h("button", { class: "button button-quiet", type: "button" }, t("cancel"));
  const editor = h("div", { class: "note-editor" }, field, status, h("div", { class: "actions" }, cancel, save));
  const actions = figure.querySelector(".hl-actions");
  actions.replaceWith(editor);
  field.focus();
  cancel.addEventListener("click", () => editor.replaceWith(actions));
  save.addEventListener("click", async () => {
    save.disabled = true;
    const saved = await changeAnnotation(annotation, { note: field.value }, status);
    if (!saved) save.disabled = false;
  });
}

async function sendAnnotation(method, annotation, changes) {
  const response = await fetch(`/api/annotations/${encodeURIComponent(annotation.id)}`, {
    method,
    headers: { Authorization: `Bearer ${currentToken}`, ...(changes && { "Content-Type": "application/json" }) },
    body: changes && JSON.stringify(changes),
  }).catch(() => null);
  if (response?.status === 409) throw new Error(t("bookOpenOnReader"));
  if (response?.status === 404) throw new Error(t("highlightMissing"));
  if (!response?.ok) throw new Error(t("editFailed"));
}

async function changeAnnotation(annotation, changes, status) {
  try {
    await sendAnnotation("PATCH", annotation, changes);
    await reloadKeepingBook();
    return true;
  } catch (error) {
    if (status) status.textContent = error.message;
    else notify(error.message);
    return false;
  }
}

function confirmAnnotationDelete(annotation) {
  const isBookmark = annotation.kind === "bookmark";
  const message = isBookmark || !annotation.text
    ? t("deleteBookmarkMessage")
    : t(annotation.note ? "deleteHighlightWithNote" : "deleteHighlightMessage", { excerpt: excerpt(annotation.text) });
  confirmAction({
    title: t(isBookmark ? "deleteBookmarkTitle" : "deleteHighlightTitle"),
    message,
    action: async () => {
      await sendAnnotation("DELETE", annotation);
      notify(t(isBookmark ? "bookmarkDeleted" : "highlightDeleted"));
      await reloadKeepingBook();
    },
  });
}

async function reloadKeepingBook() {
  const dialog = $("book-dialog");
  const openBookId = dialog.open ? dialog.dataset.bookId : null;
  await load(currentToken);
  const book = openBookId && state.books.find((b) => b.id === openBookId);
  if (book) openBook(book, { keepOpen: true });
}

function renderHighlights() {
  const byBook = Map.groupBy(state.annotations.filter((a) => a.kind !== "markup"), (a) => a.book_id);
  const groups = [...byBook].map(([bookId, items]) => {
    const book = state.books.find((b) => b.id === bookId);
    const title = book?.title ?? items[0].book_title ?? t("unknownBook");
    return h("article", { class: "hl-book" },
      h("h2", {}, h("span", { dir: "auto" }, title), h("small", {}, [book?.author, t("passages", { count: items.length })].filter(Boolean).join(", "))),
      items.map(annotationItem));
  });
  $("highlights").replaceChildren(...(groups.length ? groups : [h("p", { class: "empty" }, t("emptyHighlights"))]));
}

function renderShelves() {
  const shelves = state.shelves.map((shelf) => {
    const books = shelf.book_ids.map((id) => state.books.find((b) => b.id === id)).filter(Boolean);
    return h("section", { class: "shelf" },
      h("h2", {}, h("span", { dir: "auto" }, shelf.name), h("small", {}, t("bookCount", { count: books.length }))),
      h("ul", { class: "books" }, books.map(bookCard)));
  });
  $("shelves").replaceChildren(...(shelves.length ? shelves : [h("p", { class: "empty" }, t("emptyShelves"))]));
}

function renderStats(stats) {
  const item = (key, count) => h("span", {}, tStrong(key, { count }));
  const time = duration(stats.seconds_spent_reading);
  $("stats").replaceChildren(
    item("statBooks", stats.books),
    item("statReading", stats.reading),
    item("statFinished", stats.finished),
    item("statAnnotations", stats.annotations),
    time && h("span", {}, tStrong("statTime", { duration: time })));
}

function openBook(book, { keepOpen = false } = {}) {
  const dialog = $("book-dialog");
  dialog.dataset.bookId = book.id;
  const annotations = state.annotations.filter((a) => a.book_id === book.id && a.kind !== "markup");
  const facts = [
    t(STATUS_KEYS[book.status]) + (book.status === "reading" ? `, ${percent(book.percent_read)}` : ""),
    book.series && (book.series_number ? t("seriesNumber", { series: book.series, number: book.series_number }) : book.series),
    duration(book.seconds_spent_reading) && t("readingTime", { duration: duration(book.seconds_spent_reading) }),
  ].filter(Boolean);
  dialog.replaceChildren(
    h("div", { class: "dialog-head" },
      cover(book, { full: true }),
      h("div", { class: "dialog-title" },
        h("h2", { id: "book-dialog-title", dir: "auto", lang: book.language, tabindex: -1, autofocus: true }, book.title),
        book.author && h("p", { dir: "auto" }, book.author)),
      h("button", { class: "close", type: "button", "aria-label": t("close"), onclick: () => dialog.close() }, "×"),
      h("div", { class: "dialog-meta" },
        h("p", {}, facts.join(", ")),
        book.has_file && h("div", { class: "actions" },
          h("a", { class: "button", href: authorized(`/api/books/file?id=${encodeURIComponent(book.id)}`), download: true }, t("download")),
          h("button", { class: "button button-quiet", type: "button", onclick: () => confirmDelete(book) }, t("deleteFromReader"))))),
    h("div", { class: "dialog-body" },
      annotations.length ? annotations.map(annotationItem) : h("p", { class: "empty" }, t("noBookHighlights"))));
  if (!keepOpen || !dialog.open) dialog.showModal();
}

function confirmAction({ title, message, action }) {
  const dialog = $("confirm-dialog");
  const cancel = h("button", { class: "button button-quiet", type: "button", onclick: () => dialog.close() }, t("cancel"));
  const confirm = h("button", { class: "button button-danger", type: "button" }, t("delete"));
  const status = h("p", { class: "error", role: "alert" });
  confirm.addEventListener("click", async () => {
    confirm.disabled = true;
    try {
      await action();
      dialog.close();
    } catch (error) {
      status.textContent = error.message;
      confirm.disabled = false;
    }
  });
  dialog.replaceChildren(h("h2", { id: "confirm-title" }, title), h("p", {}, message), status, h("div", { class: "actions" }, cancel, confirm));
  dialog.showModal();
  cancel.focus();
}

function confirmDelete(book) {
  confirmAction({
    title: t("deleteBookTitle", { title: book.title }),
    message: t(book.sideloaded ? "deleteSideloaded" : "deletePurchased"),
    action: async () => {
      const response = await fetch(`/api/books?id=${encodeURIComponent(book.id)}`, {
        method: "DELETE",
        headers: { Authorization: `Bearer ${currentToken}` },
      }).catch(() => null);
      if (!response?.ok) throw new Error(t("deleteFailed"));
      $("book-dialog").close();
      notify(t("bookDeleted", { title: book.title }));
      refreshAfterImport();
    },
  });
}

// Adding books

const UPLOAD_ERRORS = { 400: "uploadInvalidName", 413: "uploadTooLarge", 415: "uploadUnsupported", 422: "uploadConversionFailed" };

function uploadBook(file, toKepub, onProgress) {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open("POST", `/api/books?name=${encodeURIComponent(file.name)}&kepub=${toKepub}`);
    request.setRequestHeader("Authorization", `Bearer ${currentToken}`);
    request.upload.addEventListener("progress", (event) => {
      if (event.lengthComputable) onProgress(event.loaded / event.total);
    });
    request.addEventListener("load", () => {
      if (request.status === 200) resolve(JSON.parse(request.responseText));
      else reject(new Error(t(UPLOAD_ERRORS[request.status] ?? "uploadError")));
    });
    request.addEventListener("error", () => reject(new Error(t("uploadConnectionLost"))));
    request.send(file);
  });
}

async function addBooks(files) {
  const toKepub = $("to-kepub").checked;
  const list = $("uploads");
  const rows = files.map((file) => {
    const bar = h("progress", { max: 1, value: 0, "aria-label": file.name });
    const label = h("span", {}, t("uploadWaiting"));
    list.append(h("li", {}, h("strong", { dir: "auto" }, file.name), bar, label));
    return { file, bar, label };
  });
  let added = 0;
  for (const { file, bar, label } of rows) {
    try {
      label.textContent = t("uploadSending");
      const willConvert = toKepub && /(?<!\.kepub)\.epub$/i.test(file.name);
      const result = await uploadBook(file, toKepub, (ratio) => {
        bar.value = ratio;
        if (ratio === 1) label.textContent = t(willConvert ? "uploadConverting" : "uploadSaving");
      });
      bar.value = 1;
      label.textContent = t(result.converted ? "uploadAddedKepub" : "uploadAdded", { name: result.name });
      added += 1;
    } catch (error) {
      bar.remove();
      label.textContent = t("uploadFailed", { reason: error.message });
      label.classList.add("error");
    }
  }
  if (added) {
    notify(t("booksAdded", { count: added }));
    refreshAfterImport();
  }
}

function setupUploads() {
  const input = $("book-files");
  $("add-books").addEventListener("click", () => input.click());
  input.addEventListener("change", () => {
    addBooks([...input.files]);
    input.value = "";
  });
  const zone = $("panel-library");
  zone.addEventListener("dragover", (event) => {
    if (!event.dataTransfer?.types.includes("Files")) return;
    event.preventDefault();
    zone.classList.add("dropping");
  });
  zone.addEventListener("dragleave", (event) => {
    if (!zone.contains(event.relatedTarget)) zone.classList.remove("dropping");
  });
  zone.addEventListener("drop", (event) => {
    event.preventDefault();
    zone.classList.remove("dropping");
    if (event.dataTransfer?.files.length) addBooks([...event.dataTransfer.files]);
  });
}

let notifyTimer;
function notify(message) {
  const toast = $("toast");
  toast.textContent = message;
  toast.hidden = false;
  clearTimeout(notifyTimer);
  notifyTimer = setTimeout(() => { toast.hidden = true; }, 6000);
}

// Nickel imports added or removed files a few seconds after the daemon asks for it.
function refreshAfterImport() {
  for (const delay of [4000, 12000]) setTimeout(() => load(currentToken).catch(() => {}), delay);
}

// Navigation

function setupTabs() {
  const tabs = [...document.querySelectorAll('[role="tab"]')];
  const select = (tab) => {
    for (const other of tabs) {
      const selected = other === tab;
      other.setAttribute("aria-selected", selected);
      other.tabIndex = selected ? 0 : -1;
      $(other.getAttribute("aria-controls")).hidden = !selected;
    }
    tab.focus();
  };
  tabs.forEach((tab, i) => {
    tab.addEventListener("click", () => select(tab));
    tab.addEventListener("keydown", (event) => {
      const forward = document.documentElement.dir === "rtl" ? "ArrowLeft" : "ArrowRight";
      const backward = forward === "ArrowLeft" ? "ArrowRight" : "ArrowLeft";
      const target = { [forward]: tabs[(i + 1) % tabs.length], [backward]: tabs[(i - 1 + tabs.length) % tabs.length], Home: tabs[0], End: tabs.at(-1) }[event.key];
      if (target) {
        event.preventDefault();
        select(target);
      }
    });
  });
}

function setupFilters() {
  $("search").addEventListener("input", (event) => {
    state.query = event.target.value.trim();
    renderBooks();
  });
  for (const button of document.querySelectorAll("[data-status]")) {
    button.addEventListener("click", () => {
      state.filter = button.dataset.status;
      for (const other of document.querySelectorAll("[data-status]")) other.setAttribute("aria-pressed", other === button);
      renderBooks();
    });
  }
  for (const id of ["book-dialog", "confirm-dialog"]) {
    $(id).addEventListener("click", (event) => {
      if (event.target === event.currentTarget) event.currentTarget.close();
    });
  }
}

// Session

function showGate(message = "") {
  $("loading").hidden = true;
  $("app").hidden = true;
  $("gate").hidden = false;
  $("gate-error").textContent = message;
  $("gate-code").focus();
}

async function load(auth) {
  const [device, stats, books, annotations, shelves] = await Promise.all(
    ["/device", "/stats", "/books", "/annotations", "/shelves"].map((path) => api(path, auth)));
  currentToken = auth;
  Object.assign(state, { books, annotations, shelves });
  renderDevice(device, true);
  renderResume();
  renderBooks();
  renderHighlights();
  renderShelves();
  renderStats(stats);
  $("loading").hidden = true;
  $("gate").hidden = true;
  $("app").hidden = false;
}

async function start(auth) {
  if (!auth) return showGate();
  try {
    await load(auth);
  } catch (error) {
    if (error instanceof Unauthorized) {
      storage.remove(TOKEN_KEY);
      return showGate(t("revoked"));
    }
    renderDevice({}, false);
    showGate(t("unreachable"));
  }
}

async function pairAndStart(code) {
  try {
    const token = await pair(code);
    storage.set(TOKEN_KEY, token);
    $("gate-form").reset();
    start(token);
  } catch (error) {
    showGate(error.message);
  }
}

function codeFromLink() {
  const code = new URLSearchParams(location.hash.slice(1)).get("code");
  if (code) history.replaceState(null, "", location.pathname);
  return code;
}

await initI18n("web");
languagePicker($("language"));

$("gate-form").addEventListener("submit", (event) => {
  event.preventDefault();
  pairAndStart($("gate-code").value.trim());
});

if ("serviceWorker" in navigator && window.isSecureContext) {
  navigator.serviceWorker.register("/sw.js").catch(() => {});
}

setupTabs();
setupFilters();
setupUploads();
window.addEventListener("hashchange", () => {
  const code = codeFromLink();
  if (code) pairAndStart(code);
});
const linkCode = codeFromLink();
if (linkCode) pairAndStart(linkCode);
else start(storage.get(TOKEN_KEY));
