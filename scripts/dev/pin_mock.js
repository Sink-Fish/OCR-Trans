// Injected before the pin page loads (scripts/check_pin_frame.sh): a minimal fake Tauri bridge that serves a test
// image as pin 1 and records the window frame the page asks for, so webkit_snap (FIT=1) can resize to exactly that
// window — the clipping the real pin window does — before taking the snapshot.
//   ?img=white|mint   the pinned image: white or light-green background with some text lines
//   &bg=<css colour>  what is behind the (transparent) pin window, i.e. the desktop
//   &size=w,h         image size in CSS px; &at=x,y where it was captured (screen coordinates, CSS px)
//   &busy=1           with &translate=1: the translation never finishes, so the 翻译中… pill stays on the image
(() => {
  const cbs = {}; let n = 1;
  const q = new URLSearchParams(location.search);
  const dpr = devicePixelRatio;
  const [iw, ih] = (q.get("size") || "320,60").split(",").map(Number);
  let pos = (q.get("at") || "400,300").split(",").map(Number); // window position (logical)
  let size = null; // last set_size (logical)
  const SCREEN = { w: 1440, h: 900, menu: 25 };
  const logical = (v) => {
    const j = JSON.parse(JSON.stringify(v)); // Size / Position → { Logical: {...} } or { width, height }
    return j.Logical || j.Physical || j.size || j.position || j;
  };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "pin-1" }, currentWebview: { windowLabel: "pin-1", label: "pin-1" } },
    transformCallback(cb) { const id = n++; cbs[id] = cb; return id; },
    unregisterCallback(id) { delete cbs[id]; },
    convertFileSrc: (p) => p,
    async invoke(cmd, args) {
      switch (cmd) {
        case "plugin:event|listen": return n++;
        case "env_info": return {};
        case "pin_info": return { width: Math.round(iw * dpr), height: Math.round(ih * dpr), scale: dpr, font_family: "sans-serif" };
        case "plugin:window|scale_factor": return dpr;
        case "plugin:window|outer_position": return { x: pos[0] * dpr, y: pos[1] * dpr };
        case "plugin:window|current_monitor":
          return {
            name: "mock", scaleFactor: dpr,
            position: { x: 0, y: 0 }, size: { width: SCREEN.w * dpr, height: SCREEN.h * dpr },
            workArea: { position: { x: 0, y: SCREEN.menu * dpr }, size: { width: SCREEN.w * dpr, height: (SCREEN.h - SCREEN.menu) * dpr } },
          };
        case "plugin:window|set_size": { const s = logical(args.value); size = [s.width, s.height]; return null; }
        case "plugin:window|set_position": { const p = logical(args.value); pos = [p.x, p.y]; return null; }
        case "pin_process": if (q.get("busy")) return new Promise(() => {}); return null;
        default: return null;
      }
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };

  // the pinned image (device pixels, like a real capture)
  const testImage = async () => {
    const c = document.createElement("canvas");
    c.width = Math.round(iw * dpr); c.height = Math.round(ih * dpr);
    const g = c.getContext("2d");
    g.fillStyle = q.get("img") === "mint" ? "#dff3e7" : "#ffffff";
    g.fillRect(0, 0, c.width, c.height);
    g.fillStyle = "#3a3f3c";
    g.font = `${14 * dpr}px -apple-system, sans-serif`;
    for (let y = 22; y < ih - 8; y += 22) g.fillText("Pinned text line — 钉图测试", 14 * dpr, y * dpr);
    return new Promise((res) => c.toBlob(res, "image/png"));
  };
  const realFetch = window.fetch;
  window.fetch = async (u, o) => (String(u).includes("/pin/") ? new Response(await testImage()) : realFetch(u, o));

  // what is behind the transparent window
  const bg = q.get("bg") || "#ffffff";
  document.addEventListener("DOMContentLoaded", () => {
    const st = document.createElement("style");
    // offscreen WebKit windows do not advance CSS animations: show the pin as it ends up (the pop-in would stay
    // at its first frame, scaled down and transparent)
    st.textContent = `html, body { background: ${bg} !important; } *, *::before, *::after { animation: none !important; transition: none !important; }`;
    document.head.appendChild(st);
  });

  // read by webkit_snap: the window size the page asked for, then where the image and its frame ended up, and what
  // sits on top of the image (the frame must not be drawn over it)
  window.__fit = () => (size ? `${size[0]},${size[1]}` : "");
  window.__report = () => {
    const box = (e) => { const b = e.getBoundingClientRect(); return [b.left, b.top, b.width, b.height]; };
    const r = (sel) => { const e = document.querySelector(sel); return e ? box(e) : null; };
    const covers = [...document.querySelectorAll(".pin-toolbar, .pin-busy, .pin-toast, .pin-tip, .pin-badge")].map((e) => ({
      cls: e.className.split(" ")[0], rect: box(e), radius: parseFloat(getComputedStyle(e).borderTopLeftRadius) || 0,
    }));
    return JSON.stringify({ dpr, window: size, stage: r(".pin-stage"), frame: r(".pin-frame"), covers, vw: innerWidth, vh: innerHeight });
  };
})();
