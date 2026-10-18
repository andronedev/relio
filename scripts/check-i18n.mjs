// Checks every i18n/<lang>.json against i18n/en.json: same keys, same placeholders,
// and every plural category CLDR uses for counts of 0 to 1000 in that language.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const dir = join(import.meta.dirname, "..", "i18n");
const load = (file) => JSON.parse(readFileSync(join(dir, file), "utf8"));
const source = load("en.json");
const placeholders = (text) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");

function* entries(object, prefix = "") {
  for (const [key, value] of Object.entries(object)) {
    const path = prefix ? `${prefix}.${key}` : key;
    const isPlural = value && typeof value === "object" && "other" in value;
    if (value && typeof value === "object" && !isPlural) yield* entries(value, path);
    else yield [path, value];
  }
}

const sourceEntries = new Map(entries(source));
// Must match PLURAL_LOCALES in web/i18n.js.
const PLURAL_LOCALES = { pt: "pt-PT" };
let failures = 0;
const fail = (lang, message) => {
  failures += 1;
  console.error(`${lang}: ${message}`);
};

for (const file of readdirSync(dir).filter((f) => f.endsWith(".json")).sort()) {
  const lang = file.slice(0, -5);
  const target = new Map(entries(load(file)));
  const rules = new Intl.PluralRules(PLURAL_LOCALES[lang] ?? lang);
  const allowed = rules.resolvedOptions().pluralCategories;
  const categories = [...new Set(Array.from({ length: 1001 }, (_, n) => rules.select(n)))];
  if (!["ltr", "rtl"].includes(target.get("meta.dir"))) fail(lang, "meta.dir must be ltr or rtl");

  for (const [key, expected] of sourceEntries) {
    const actual = target.get(key);
    if (actual === undefined) {
      fail(lang, `missing ${key}`);
      continue;
    }
    if (key.startsWith("meta.")) continue;
    const expectedPlaceholders = placeholders(typeof expected === "string" ? expected : expected.other);
    if (typeof expected === "string") {
      if (typeof actual !== "string") fail(lang, `${key} must be a string`);
      else if (placeholders(actual) !== expectedPlaceholders) fail(lang, `${key} placeholders differ`);
      continue;
    }
    if (typeof actual !== "object") {
      fail(lang, `${key} must be a plural object`);
      continue;
    }
    for (const category of categories) {
      if (typeof actual[category] !== "string") fail(lang, `${key} lacks plural "${category}"`);
    }
    for (const [category, text] of Object.entries(actual)) {
      if (!allowed.includes(category)) fail(lang, `${key} has unknown plural "${category}"`);
      else if (placeholders(text).replace("count", "") !== expectedPlaceholders.replace("count", "")) {
        fail(lang, `${key}.${category} placeholders differ`);
      }
    }
  }
  for (const key of target.keys()) {
    if (!sourceEntries.has(key)) fail(lang, `unknown key ${key}`);
  }
}

if (failures) {
  console.error(`${failures} problem(s)`);
  process.exit(1);
}
console.log("i18n ok");
