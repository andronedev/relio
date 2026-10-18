// Loads the visitor's language from i18n/*.json and formats messages with
// plural rules, numbers and text direction for that language.

const STORAGE_KEY = "relio.lang";
const FALLBACK = "en";
// CLDR keys plain "pt" to Brazilian rules; pt.json is European Portuguese.
const PLURAL_LOCALES = { pt: "pt-PT" };

let lang = FALLBACK;
let messages = {};
let fallbackMessages = {};
let languages = {};

const storage = {
  get: () => { try { return localStorage.getItem(STORAGE_KEY); } catch { return null; } },
  set: (value) => { try { localStorage.setItem(STORAGE_KEY, value); } catch { /* choice lasts for this page */ } },
};

function normalize(tag, available) {
  const lower = new Map(available.map((code) => [code.toLowerCase(), code]));
  const [base, ...rest] = tag.toLowerCase().split("-");
  const region = rest.join("-");
  if (base === "zh") return /hant|tw|hk|mo/.test(region) ? lower.get("zh-hant") : lower.get("zh-hans");
  if (base === "no" || base === "nn") return lower.get("nb");
  return lower.get(tag.toLowerCase()) ?? (region === "br" ? undefined : lower.get(base));
}

function pick(available) {
  const candidates = [storage.get(), ...(navigator.languages ?? [navigator.language])].filter(Boolean);
  for (const tag of candidates) {
    const match = normalize(tag, available) ?? (tag.toLowerCase().startsWith("pt") ? normalize("pt", available) : undefined);
    if (match) return match;
  }
  return FALLBACK;
}

async function fetchLocale(code, base) {
  const response = await fetch(`${base}i18n/${code}.json`);
  if (!response.ok) throw new Error(`locale ${code}: ${response.status}`);
  return response.json();
}

// `sections` are merged in order, the first one winning on duplicate keys.
export async function initI18n(sections, base = "/") {
  languages = await fetch(`${base}i18n/index.json`).then((r) => r.json()).catch(() => ({ en: { name: "English", dir: "ltr" } }));
  lang = pick(Object.keys(languages));
  const [current, english] = await Promise.all([
    fetchLocale(lang, base).catch(() => null),
    lang === FALLBACK ? null : fetchLocale(FALLBACK, base).catch(() => null),
  ]);
  if (!current) lang = FALLBACK;
  const merge = (locale) => Object.assign({}, ...[sections].flat().reverse().map((name) => locale?.[name] ?? {}));
  messages = merge(current ?? english);
  fallbackMessages = english ? merge(english) : messages;
  document.documentElement.lang = lang;
  document.documentElement.dir = languages[lang]?.dir ?? "ltr";
  translateDocument();
}

export const locale = () => lang;

function formatValue(value) {
  return typeof value === "number" ? new Intl.NumberFormat(lang).format(value) : String(value);
}

export function t(key, params = {}) {
  let text = messages[key] ?? fallbackMessages[key] ?? key;
  if (typeof text === "object") {
    const category = new Intl.PluralRules(PLURAL_LOCALES[lang] ?? lang).select(params.count ?? 0);
    text = text[category] ?? text.other;
  }
  return text.replace(/\{(\w+)\}/g, (match, name) => (name in params ? formatValue(params[name]) : match));
}

// The same message, with each placeholder value emphasised as its own element.
export function tStrong(key, params = {}) {
  const parts = [];
  let text = t(key, Object.fromEntries(Object.keys(params).map((name) => [name, `\u0000${name}\u0000`])));
  for (const [index, piece] of text.split("\u0000").entries()) {
    if (!piece) continue;
    if (index % 2 === 1 && piece in params) {
      const strong = document.createElement("b");
      strong.textContent = formatValue(params[piece]);
      parts.push(strong);
    } else {
      parts.push(piece);
    }
  }
  return parts;
}

// Elements carry data-i18n="key" for their text and data-i18n-attr="attr:key; attr:key" for attributes.
export function translateDocument(root = document) {
  for (const element of root.querySelectorAll("[data-i18n]")) element.textContent = t(element.dataset.i18n);
  for (const element of root.querySelectorAll("[data-i18n-attr]")) {
    for (const pair of element.dataset.i18nAttr.split(";")) {
      const [attribute, key] = pair.split(":").map((part) => part.trim());
      if (attribute && key) element.setAttribute(attribute, t(key));
    }
  }
}

export function languagePicker(select) {
  const collator = new Intl.Collator(lang);
  const entries = Object.entries(languages).sort((a, b) => collator.compare(a[1].name, b[1].name));
  select.replaceChildren(...entries.map(([code, meta]) => {
    const option = document.createElement("option");
    option.value = code;
    option.lang = code;
    option.textContent = meta.name;
    option.selected = code === lang;
    return option;
  }));
  select.addEventListener("change", () => {
    storage.set(select.value);
    location.reload();
  });
}
