import { invoke } from "@tauri-apps/api/core";

export type Quad = [[number, number], [number, number], [number, number], [number, number]];

export interface TextLine {
  quad: Quad;
  text: string;
  score: number;
}

export interface Paragraph {
  bbox: [number, number, number, number];
  lines: TextLine[];
  text: string;
  line_height: number;
  lang: "en" | "ja" | "zh" | "other";
  bg: string;
  fg: string;
  translation: string | null;
  fill: "inpaint" | "translucent" | null;
}

export interface PipelineResult {
  width: number;
  height: number;
  paragraphs: Paragraph[];
  timings: Record<string, number>;
  warnings: string[];
  engine: string;
}

export interface PinInfo {
  has_saved_translation: boolean;
  physical_anchor: [number, number];
  width: number;
  height: number;
  scale: number;
  font_family: string;
}

export interface Settings {
  version: number;
  hotkey_capture: string;
  auto_translate: boolean;
  default_action: "translate" | "ocr" | "pin";
  hotkey_pin: string;
  hotkey_ocr: string;
  hotkey_translate: string;
  models_dir: string | null;
  mirror: string;
  font_family: string;
  /** Copy all recognised text as soon as 识字 finishes (off by default: copy only when asked). */
  ocr_auto_copy: boolean;
  engine: "fast" | "quality";
  glossary: [string, string][];
  onboarded: boolean;
  /** keep the last 50 captures in the history folder */
  keep_history: boolean;
  history_days: number;
  history_mb: number;
  /** hardware for the quality model: "auto" (a usable GPU, else the CPU) or "cpu" */
  llm_device: "auto" | "cpu";
  /** why the GPU is not used any more (it failed once); null = it may be used */
  llm_gpu_failed: string | null;
  /** release the quality model after this many idle minutes (0 = keep it) */
  llm_idle_minutes: number;
}

export interface LlmInfo {
  /** what the quality model ran on last; null until it was loaded in this run */
  running_on: string | null;
  gpu_failed: string | null;
  cpu: string;
}

export interface HistoryItem {
  id: number;
  created: number;
  width: number;
  height: number;
  scale: number;
  rect: [number, number, number, number];
  mode: string;
  text: string;
  translation: string;
}

export interface ModelStatus {
  id: string;
  name: string;
  description: string;
  license: string;
  approx_mb: number;
  installed: boolean;
  verified: boolean;
  path: string | null;
  size_mb: number;
  deletable: boolean;
  downloading: boolean;
}

export const isWindows = navigator.userAgent.includes("Windows");

/** URL of an image served by the Rust `shot` protocol. */
export function shotUrl(path: string): string {
  return (isWindows ? "http://shot.localhost/" : "shot://localhost/") + path;
}

export async function loadBitmap(path: string): Promise<ImageBitmap> {
  const r = await fetch(shotUrl(path), { cache: "no-store" });
  if (!r.ok) throw new Error(`load ${path}: ${r.status}`);
  return createImageBitmap(await r.blob());
}

export const log = (msg: string) => invoke("log", { msg }).catch(() => {});

export const api = {
  overlayReady: (token: number, fetchMs?: number, paintMs?: number) => invoke("overlay_ready", { token, fetchMs, paintMs }),
  overlayCancel: (token?: number) => invoke("overlay_cancel", { token }),
  overlayDone: (token: number, x: number, y: number, w: number, h: number, action?: string) => invoke("overlay_done", { token, x, y, w, h, action }),
  startCapture: (mode: "select" | "pin" | "ocr" | "translate") => invoke("start_capture", { mode }),
  pinInfo: (id: number) => invoke<PinInfo>("pin_info", { id }),
  pinCancel: (id: number) => invoke<void>("pin_cancel", { id }),
  pinProcess: (id: number, translate: boolean, engine?: "fast" | "quality") => invoke<PipelineResult>("pin_process", { id, translate, engine }),
  pinSetTranslation: (id: number, index: number, text: string) => invoke("pin_set_translation", { id, index, text }),
  openPin: (id: number, translate: boolean) => invoke("open_pin", { id, translate }),
  openTextWindow: (id: number, translated: boolean) => invoke("open_text_window", { id, translated }),
  toast: (msg: string, kind: "ok" | "error" = "ok") => invoke("toast", { msg, kind }),
  copyText: (text: string) => invoke("copy_text", { text }),
  copyPng: (png: Uint8Array) => invoke("copy_png", png),
  savePng: (png: Uint8Array) => invoke<string | null>("save_png", png),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (value: Settings) => invoke<string[]>("set_settings", { value }),
  /** Save only the given fields (never write back a stale copy of the others); returns the whole new settings. */
  patchSettings: (patch: Partial<Settings>) => invoke<{ settings: Settings; errors: string[] }>("patch_settings", { patch }),
  modelVerify: (id: string) => invoke<string>("model_verify", { id }),
  modelStatus: () => invoke<ModelStatus[]>("model_status"),
  modelDownload: (id: string) => invoke("model_download", { id }),
  modelCancel: (id: string) => invoke("model_cancel", { id }),
  modelDelete: (id: string) => invoke("model_delete", { id }),
  modelsFolder: () => invoke<string>("models_folder"),
  pickFolder: () => invoke<string | null>("pick_folder"),
  envInfo: () => invoke<{ screen_access: boolean; platform: string; version: string; theme?: string | null }>("env_info"),
  /** Full text of the bundled THIRD-PARTY-NOTICES.txt. */
  saveDiagnostics: (report: string) => invoke<boolean>("save_diagnostics", { report }),
  releaseProbe: (engine: string, warnings: number) => invoke<void>("release_probe", { engine, warnings }),
  diagnostics: () => invoke<string>("diagnostics"),
  thirdPartyNotices: () => invoke<string>("third_party_notices"),
  openThirdPartyNotices: () => invoke("open_third_party_notices"),
  llmInfo: () => invoke<LlmInfo>("llm_info"),
  requestScreenAccess: () => invoke("request_screen_access"),
  restartApp: () => invoke("restart_app"),
  openSettings: (tab: string) => invoke("open_settings", { tab }),
  /** Close the current window safely (hide now, destroy later) — never use window.close() on our web views. */
  closeSelf: () => invoke("close_self"),
  openHistory: () => invoke("open_history"),
  historyRecover: () => invoke<void>("history_recover"),
  historyList: () => invoke<HistoryItem[]>("history_list"),
  historyPin: (id: number, translate: boolean) => invoke("history_pin", { id, translate }),
  historyDelete: (id: number) => invoke("history_delete", { id }),
  historyClear: () => invoke("history_clear"),
};

export function plainText(r: PipelineResult, translated: boolean): string {
  return r.paragraphs.map((p) => (translated && p.translation ? p.translation : p.text)).join("\n");
}
