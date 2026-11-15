// Injected into the production React bundle by check_frontend_behavior.sh (real macOS WebKit).
(() => {
  const scenario = new URLSearchParams(location.search).get("case");
  const callbacks = {};
  let callbackId = 0;
  const listeners = {};
  let historyRecovered = false;
  let cancelPending;
  let verified = false, exported = false;
  let settings = {
    version: 1, hotkey_capture: "Alt+Shift+O", auto_translate: false, default_action: "translate",
    hotkey_pin: "", hotkey_ocr: "", hotkey_translate: "", models_dir: null, mirror: "auto",
    font_family: "sans-serif", ocr_auto_copy: false, engine: "fast", glossary: [["Pin", "钉图"]],
    onboarded: false, keep_history: true, llm_device: "auto", llm_gpu_failed: null, llm_idle_minutes: 5,
  };
  let writes = 0, closeCalls = 0, processCalls = 0, openedBitmaps = 0, closedBitmaps = 0;
  let modelInstalled = false;
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const assert = (value, message) => { if (!value) throw new Error(message); };
  const clone = (value) => JSON.parse(JSON.stringify(value));
  const result = { passed: false, scenario, checks: [] };
  window.__fit = () => "900,750";
  window.__report = () => JSON.stringify(result);
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: scenario }, currentWebview: { windowLabel: scenario, label: scenario } },
    transformCallback(cb) { const id = ++callbackId; callbacks[id] = cb; return id; },
    unregisterCallback(id) { delete callbacks[id]; },
    convertFileSrc: (path) => path,
    async invoke(command, args) {
      switch (command) {
        case "plugin:event|listen": listeners[args.event] = args.handler; return ++callbackId;
        case "history_list":
          if (!historyRecovered) throw new Error("历史索引损坏");
          return [{id: 1, created: 1, width: 3, height: 2, scale: 1, mode: "ocr", text: "恢复的文字", translation: ""}];
        case "history_recover":
          historyRecovered = true;
          callbacks[listeners["history-changed"]]({event:"history-changed", id: 1, payload:null});
          return null;
        case "get_settings": return clone(settings);
        case "env_info": return { platform: "macos", screen_access: true, version: "test" };
        case "model_status": return [{ id: "ocr", name: "OCR", description: "", license: "", size_mb: 20,
          approx_mb: 20, path: "/models/ocr", installed: scenario === "models" || modelInstalled, verified }];
        case "model_verify": verified = true; return "校验通过";
        case "diagnostics": return "test diagnostics preview";
        case "save_diagnostics": exported = args.report === "test diagnostics preview"; return true;
        case "llm_info": return null;
        case "plugin:autostart|is_enabled": return false;
        case "patch_settings":
          writes++;
          await sleep(200);
          if (scenario === "onboarding") throw new Error("simulated disk failure");
          settings = { ...settings, ...clone(args.patch) };
          return { settings: clone(settings), errors: [] };
        case "close_self": closeCalls++; return null;
        case "pin_info": return { width: 320, height: 120, scale: 1, font_family: "sans-serif" };
        case "plugin:window|scale_factor": return 1;
        case "plugin:window|outer_position": return { x: 100, y: 100 };
        case "plugin:window|current_monitor": return {
          name: "mock", scaleFactor: 1, position: { x: 0, y: 0 }, size: { width: 1440, height: 900 },
          workArea: { position: { x: 0, y: 25 }, size: { width: 1440, height: 875 } },
        };
        case "pin_cancel":
          if (cancelPending) cancelPending(new Error("翻译已取消"));
          return null;
        case "pin_process":
          processCalls++;
          if (scenario === "pin" && processCalls === 4) {
            await new Promise((_, reject) => { cancelPending = reject; });
          }
          await sleep(30);
          return {
            width: 320, height: 120, engine: args.engine === "quality" ? "llm" : "nmt", warnings: [], timings: {},
            paragraphs: [{ bbox: [10, 10, 100, 20], lines: [], text: "Hello", translation: "你好", line_height: 20, fill: "inpaint", bg: "#ffffff", fg: "#000000", lang: "en" }],
          };
        default: return null;
      }
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  const originalCreate = window.createImageBitmap.bind(window);
  const originalClose = ImageBitmap.prototype.close;
  window.createImageBitmap = async (...args) => { const bitmap = await originalCreate(...args); openedBitmaps++; return bitmap; };
  ImageBitmap.prototype.close = function () { closedBitmaps++; return originalClose.call(this); };
  const originalFetch = window.fetch.bind(window);
  window.fetch = async (url, options) => {
    if (!String(url).includes("/pin/")) return originalFetch(url, options);
    const canvas = document.createElement("canvas");
    canvas.width = 320; canvas.height = 120;
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#ffffff"; ctx.fillRect(0, 0, 320, 120);
    return new Response(await new Promise((resolve) => canvas.toBlob(resolve, "image/png")));
  };
  async function waitFor(predicate, label) {
    for (let attempt = 0; attempt < 100; attempt++) {
      const value = predicate();
      if (value) return value;
      await sleep(20);
    }
    throw new Error(`timed out: ${label}`);
  }
  const button = (label) => [...document.querySelectorAll("button")].find((el) => el.textContent.trim() === label);
  function type(editor, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(editor, text);
    editor.dispatchEvent(new Event("input", { bubbles: true }));
  }
  document.addEventListener("DOMContentLoaded", async () => {
    try {
      if (scenario === "settings") {
        const editor = await waitFor(() => document.querySelector("textarea.glossary"), "glossary editor");
        button("高质量").click();
        editor.focus();
        type(editor, "Equation = a = b → c");
        await sleep(300);
        assert(editor.value === "Equation = a = b → c", "an unrelated settings reply overwrote the glossary draft");
        result.checks.push("glossary draft survives an unrelated settings reply");
        editor.blur();
        await sleep(300);
        assert(settings.glossary[0][1] === "a = b → c", "translation separators were lost");
        result.checks.push("glossary saves equals signs and arrows unchanged");
        const before = writes;
        editor.focus(); type(editor, "Equation = a = b → c\nincomplete"); editor.blur();
        await sleep(300);
        assert(writes === before && document.querySelector('[role="alert"]'), "invalid glossary silently replaced saved terms");
        result.checks.push("invalid glossary preserves saved terms and shows an error");
      } else if (scenario === "models") {
        (await waitFor(() => button("校验文件"), "model verification action")).click();
        await waitFor(() => document.querySelector(".model .tag")?.textContent.includes("已校验"), "verified status");
        result.checks.push("model verification refreshes the status");
      } else if (scenario === "about") {
        (await waitFor(() => button("查看诊断"), "diagnostics action")).click();
        await waitFor(() => document.querySelector("pre")?.textContent === "test diagnostics preview", "diagnostics preview");
        button("导出诊断文件").click();
        await waitFor(() => exported, "matching exported content");
        result.checks.push("export uses the reviewed diagnostic text");
      } else if (scenario === "history") {
        const restore = await waitFor(() => button("恢复历史索引"), "recovery action");
        assert(document.querySelector('[role="alert"]').textContent.includes("索引损坏"), "read failure hidden");
        restore.click();
        await waitFor(() => document.querySelector(".snippet")?.textContent.includes("恢复的文字"), "restored list");
        result.checks.push("damaged history is visible and the recovery button restores the list");
      } else if (scenario === "pin") {
        await waitFor(() => document.querySelector(".pin-badge"), "initial translation");
        document.querySelector('button[title="更多"]').click();
        await waitFor(() => button("高质量重译"), "retranslation control");
        button("高质量重译").click();
        await waitFor(() => button("改用快速翻译") && closedBitmaps === 1, "first old bitmap released");
        button("改用快速翻译").click();
        await waitFor(() => button("高质量重译") && closedBitmaps === 2, "second old bitmap released");
        assert(processCalls === 3 && openedBitmaps === 4 && closedBitmaps === 2, "retranslation leaked decoded bitmaps");
        result.checks.push("three translations retain only the source and latest cleaned bitmap");
        result.bitmaps = { opened: openedBitmaps, closed: closedBitmaps };
        button("高质量重译").click();
        const cancel = await waitFor(() => button("取消"), "cancel translation action");
        cancel.click();
        await waitFor(() => !document.querySelector(".pin-busy"), "cancelled translation clears busy state");
        assert(document.querySelector(".pin-badge"), "cancellation lost previous result");
        result.checks.push("cancelling retranslation keeps the previous result and releases busy state");
      } else if (scenario === "onboarding") {
        await waitFor(() => button("开始使用"), "onboarding");
        button("开始使用").click();
        await sleep(300);
        assert(closeCalls === 0 && document.querySelector('[role="alert"]'), "failed onboarding save still closed the window");
        result.checks.push("failed onboarding save stays open and shows an error");
        modelInstalled = true;
        await waitFor(() => [...document.querySelectorAll(".model-line")].some((el) => el.textContent.includes("文字识别") && el.textContent.includes("已就绪")), "model readiness refresh");
        result.checks.push("onboarding refreshes models downloaded in another window");
      }
      result.passed = true;
    } catch (error) { result.error = String(error); }
  });
})();
