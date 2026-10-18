import { initI18n, languagePicker, t } from "./i18n.js";

const PACKAGE_URL = "KoboRoot.tgz";
const MODELS = {
  "376": "Kobo Clara HD", "377": "Kobo Forma", "380": "Kobo Forma", "382": "Kobo Nia", "383": "Kobo Sage",
  "384": "Kobo Libra H2O", "386": "Kobo Clara 2E", "387": "Kobo Elipsa", "388": "Kobo Libra 2",
  "389": "Kobo Elipsa 2E", "390": "Kobo Libra Colour", "391": "Kobo Clara BW", "393": "Kobo Clara Colour",
};

const $ = (id) => document.getElementById(id);
let koboDir = null;

function setStep(id, state) {
  $(id).dataset.state = state;
}

function describe(versionFile) {
  const fields = versionFile.trim().split(",");
  const modelId = fields[5]?.split("-").pop()?.replace(/^0+/, "");
  return { model: MODELS[modelId] ?? "Kobo", firmware: fields[2] };
}

export async function readKobo(dir) {
  try {
    const kobo = await dir.getDirectoryHandle(".kobo");
    const version = await (await (await kobo.getFileHandle("version")).getFile()).text();
    return { kobo, ...describe(version) };
  } catch {
    return null;
  }
}

async function hasPendingUpdate(kobo) {
  try {
    await kobo.getFileHandle("KoboRoot.tgz");
    return true;
  } catch {
    return false;
  }
}

async function pick() {
  const status = $("pick-status");
  let dir;
  try {
    dir = await window.showDirectoryPicker({ id: "kobo", mode: "readwrite" });
  } catch (error) {
    if (error.name !== "AbortError") status.textContent = t("pickDenied");
    return;
  }

  const kobo = await readKobo(dir);
  if (!kobo) {
    status.textContent = t("pickNotKobo", { name: dir.name });
    return;
  }

  koboDir = kobo;
  status.textContent = kobo.firmware
    ? t("detectedFirmware", { model: kobo.model, firmware: kobo.firmware })
    : t("detected", { model: kobo.model });
  setStep("step-pick", "done");
  setStep("step-install", "active");
  $("install").disabled = false;
  $("install").focus();
}

export async function writePackage(kobo) {
  const response = await fetch(PACKAGE_URL, { cache: "no-store" });
  if (!response.ok) throw new Error(t("packageUnavailable", { status: response.status }));
  const file = await kobo.getFileHandle("KoboRoot.tgz", { create: true });
  await response.body.pipeTo(await file.createWritable());
}

async function install() {
  const status = $("install-status");
  const button = $("install");
  if (await hasPendingUpdate(koboDir.kobo) && !confirm(t("replacePending"))) return;

  button.disabled = true;
  status.textContent = t("copying");
  try {
    await writePackage(koboDir.kobo);
    status.textContent = t("copied");
    setStep("step-install", "done");
    setStep("step-finish", "active");
  } catch (error) {
    status.textContent = t("copyFailed", { reason: error.message });
    button.disabled = false;
  }
}

await initI18n(["install", "web"], "");
document.title = t("pageTitle");
languagePicker($("language"));

if ("showDirectoryPicker" in window && window.isSecureContext) {
  $("pick").addEventListener("click", pick);
  $("install").addEventListener("click", install);
} else {
  for (const id of ["step-pick", "step-install"]) $(id).hidden = true;
  $("fallback").hidden = false;
}
