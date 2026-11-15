// Injected before the overlay page loads (scripts/check_overlay_frame.sh): a minimal fake Tauri bridge that feeds
// the page a striped test "screenshot" and drags the selection given by ?sel=x0,y0,x1,y1.
(() => {
  const cbs = {}; let n = 1; const listeners = {};
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "overlay" }, currentWebview: { windowLabel: "overlay", label: "overlay" } },
    transformCallback(cb) { const id = n++; cbs[id] = cb; return id; },
    unregisterCallback(id) { delete cbs[id]; },
    convertFileSrc: (p) => p,
    async invoke(cmd, args) {
      if (cmd === "plugin:event|listen") { (listeners[args.event] ||= []).push(args.handler); return n++; }
      return null;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  const q = new URLSearchParams(location.search);
  const sel = (q.get("sel") || "100,80,400,230").split(",").map(Number);
  const realFetch = window.fetch;
  let pw = 0, ph = 0;
  window.fetch = async (u, o) => {
    if (String(u).includes("overlay/")) {
      const b = new Uint8ClampedArray(pw * ph * 4);
      for (let i = 0; i < b.length; i += 4) { const x = (i / 4) % pw; const g = (Math.floor(x / 40) % 2) ? 255 : 235; b[i] = g; b[i + 1] = g; b[i + 2] = g; b[i + 3] = 255; }
      return new Response(b.buffer);
    }
    return realFetch(u, o);
  };
  const emit = (ev, payload) => (listeners[ev] || []).forEach((id) => cbs[id]({ event: ev, id: 1, payload }));
  window.addEventListener("load", () => setTimeout(() => {
    pw = Math.round(innerWidth * devicePixelRatio); ph = Math.round(innerHeight * devicePixelRatio);
    emit("overlay-start", { token: 1, mode: "select", pw, ph, defaultAction: "translate" });
    setTimeout(() => {
      const el = document.querySelector(".overlay");
      const f = (t, x, y) => el.dispatchEvent(new PointerEvent(t, {
        clientX: x, clientY: y, button: 0, buttons: 1, bubbles: true, pointerId: 1, isPrimary: true,
      }));
      f("pointerdown", sel[0], sel[1]);
      setTimeout(() => { f("pointermove", sel[2], sel[3]); setTimeout(() => f("pointerup", sel[2], sel[3]), 150); }, 150);
    }, 400);
  }, 200));
})();
