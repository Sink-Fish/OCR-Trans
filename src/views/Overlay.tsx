import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { api, log, shotUrl } from "../lib/api";
import { ActionIcon, type ActionId } from "../lib/icons";
import { selectionHandles, toolbarPosition } from "../lib/placement";

type Mode = "select" | "pin" | "ocr" | "translate";
const MODE_LABEL: Record<Mode, string> = { select: "截图", pin: "截图钉图", ocr: "截图识字", translate: "截图翻译" };

interface Sel {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

type Edges = { l: boolean; r: boolean; t: boolean; b: boolean };

function resizeSelection(s: Sel, edges: Edges, x: number, y: number): Sel {
  const n = { x0: Math.min(s.x0, s.x1), y0: Math.min(s.y0, s.y1), x1: Math.max(s.x0, s.x1), y1: Math.max(s.y0, s.y1) };
  if (edges.l) n.x0 = Math.min(x, n.x1 - 4);
  if (edges.r) n.x1 = Math.max(x, n.x0 + 4);
  if (edges.t) n.y0 = Math.min(y, n.y1 - 4);
  if (edges.b) n.y1 = Math.max(y, n.y0 + 4);
  return n;
}

function localPoint(e: React.PointerEvent): [number, number] {
  return [Math.max(0, Math.min(window.innerWidth, e.clientX)), Math.max(0, Math.min(window.innerHeight, e.clientY))];
}

type DefaultAction = "translate" | "ocr" | "pin";

interface Start {
  token: number;
  mode: Mode;
  pw: number;
  ph: number;
  autoTranslate?: boolean;
  defaultAction?: DefaultAction;
  bench?: boolean;
}

const isMac = navigator.userAgent.includes("Mac");
const MOD = isMac ? "⌘" : "Ctrl+";

/** Toolbar actions shown after a selection (unified capture). `num` = number key (1–4), `key` = letter shortcut. */
const ACTIONS: { id: ActionId; label: string; key: string; num?: number }[] = [
  { id: "translate", label: "翻译", key: "T", num: 1 },
  { id: "ocr", label: "识字", key: "O", num: 2 },
  { id: "pin", label: "钉图", key: "P", num: 3 },
  { id: "doodle", label: "标注", key: "D", num: 4 },
  { id: "copy", label: "复制", key: `${MOD}C` },
  { id: "save", label: "保存", key: `${MOD}S` },
];
const BY_NUMBER: Record<string, ActionId> = { "1": "translate", "2": "ocr", "3": "pin", "4": "doodle" };

const HANDLE_CURSOR: Record<string, string> = {
  "00": "nwse-resize",
  "0.50": "ns-resize",
  "10": "nesw-resize",
  "10.5": "ew-resize",
  "11": "nwse-resize",
  "0.51": "ns-resize",
  "01": "nesw-resize",
  "00.5": "ew-resize",
};

/** Full-screen frozen screenshot; drag to select a region, then pick an action. */
export default function Overlay() {
  const [loaded, setLoaded] = useState(false);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [mode, setMode] = useState<Mode>("select");
  const [sel, setSel] = useState<Sel | null>(null);
  // a finished selection waiting for an action (unified capture with the toolbar)
  const [chosen, setChosen] = useState<Sel | null>(null);
  const [mouse, setMouse] = useState<[number, number] | null>(null);
  const opts = useRef<{ autoTranslate: boolean; defaultAction: DefaultAction }>({ autoTranslate: false, defaultAction: "translate" });
  const barRef = useRef<HTMLDivElement>(null);
  const [barSize, setBarSize] = useState<{ w: number; h: number } | undefined>(undefined);
  const dragStart = useRef<[number, number] | null>(null);
  // resizing a finished selection by one of its handles: which edges move
  const resizing = useRef<Edges | null>(null);
  const busy = useRef(false);
  // token of the screenshot currently shown; events/loads for older tokens are ignored
  const token = useRef(0);
  // mouse moves arrive faster than frames (120 Hz trackpads): coalesce into one React update per frame
  const pending = useRef<[number, number] | null>(null);
  const raf = useRef(0);
  const onMoveCoalesced = (x: number, y: number) => {
    pending.current = [x, y];
    if (raf.current) return;
    raf.current = requestAnimationFrame(() => {
      raf.current = 0;
      const p = pending.current;
      if (!p) return;
      setMouse(p);
      if (dragStart.current) setSel((s) => s && { ...s, x1: p[0], y1: p[1] });
      const rz = resizing.current;
      if (rz)
        setChosen((c) => c && resizeSelection(c, rz, ...p));
    });
  };

  useEffect(() => {
    let disposed = false;
    let lastToken = 0;
    const start = async (payload: Start) => {
      if (disposed || payload.token <= lastToken) return;
      lastToken = payload.token;
      const t0 = performance.now();
      clear();
      setMode(payload.mode);
      const d = payload.defaultAction;
      opts.current = { autoTranslate: !!payload.autoTranslate, defaultAction: d === "ocr" || d === "pin" ? d : "translate" };
      const my = payload.token;
      token.current = my;
      try {
        const r = await fetch(shotUrl(`overlay/${my}`));
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        const buf = await r.arrayBuffer();
        const t1 = performance.now();
        if (disposed || token.current !== my) return; // a newer capture arrived meanwhile
        const { pw, ph } = payload;
        const c = canvasRef.current!;
        c.width = pw;
        c.height = ph;
        c.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(buf), pw, ph), 0, 0);
        setLoaded(true);
        const t2 = performance.now();
        await api.overlayReady(my, t1 - t0, t2 - t1);
        if (payload.bench) benchMouse();
      } catch (err) {
        log(`overlay load failed: ${err}`);
        if (!disposed && token.current === my) void invoke("overlay_failed", { token: my }).catch(console.warn);
      }
    };
    const un = listen<Start>("overlay-start", (e) => { void start(e.payload); });
    // the backend cancelled the capture (Esc, another window took the focus, a failed capture, a newer capture):
    // reset the page
    const un2 = listen<number>("overlay-cancelled", ({ payload }) => {
      lastToken = Math.max(lastToken, payload);
      if (token.current !== payload) return;
      token.current = 0;
      clear();
    });
    Promise.all([un, un2]).then(async () => {
      const current = await invoke<Start | null>("overlay_current");
      if (current) await start(current);
    }).catch((e) => log(`overlay bootstrap failed: ${e}`));
    return () => {
      disposed = true;
      un.then((f) => f());
      un2.then((f) => f());
    };
  }, []);

  // keyboard: Esc always cancels; with the toolbar up, single keys pick the action. Returns true if handled.
  // Only this window's own key events: these keys are never global shortcuts (a global Enter is taken away from
  // every other app — in 0.5.6 also from macOS's screen-recording confirmation hidden behind the overlay).
  const onKey = useRef<(key: string, mod: boolean) => boolean>(() => false);
  onKey.current = (key: string, mod: boolean) => {
    if (key === "Escape") {
      cancel();
      return true;
    }
    if (!chosen) return false;
    const k = key.toLowerCase();
    let a: ActionId | null = null;
    if (!mod && BY_NUMBER[key]) a = BY_NUMBER[key];
    else if (mod && k === "c") a = "copy";
    else if (mod && k === "s") a = "save";
    else if (!mod && k === "t") a = "translate";
    else if (!mod && k === "o") a = "ocr";
    else if (!mod && k === "p") a = "pin";
    else if (!mod && k === "d") a = "doodle";
    else if (key === "Enter") a = opts.current.defaultAction;
    if (!a) return false;
    run(chosen, a);
    return true;
  };
  useEffect(() => {
    const f = (e: KeyboardEvent) => {
      if (e.isComposing) return;
      if (onKey.current(e.key, e.metaKey || e.ctrlKey)) e.preventDefault();
    };
    window.addEventListener("keydown", f);
    return () => window.removeEventListener("keydown", f);
  }, []);

  /** Dev benchmark (OCRTRANS_BENCH): 120 synthetic drag frames, then a real mouse-up through the page. */
  function benchMouse() {
    const el = document.querySelector(".overlay") as HTMLElement;
    const fire = (type: string, x: number, y: number) => el.dispatchEvent(new PointerEvent(type, {
      clientX: x, clientY: y, button: 0, buttons: 1, bubbles: true, pointerId: 1, isPrimary: true,
    }));
    let i = 0;
    let last = performance.now();
    const frames: number[] = [];
    fire("pointerdown", 100, 100);
    const step = () => {
      const now = performance.now();
      frames.push(now - last);
      last = now;
      fire("pointermove", 100 + i * 5, 100 + i * 3);
      if (++i < 120) requestAnimationFrame(step);
      else {
        frames.sort((a, b) => a - b);
        log(`[perf] overlay drag frames: median ${frames[60].toFixed(1)} ms, p95 ${frames[114].toFixed(1)} ms, max ${frames[119].toFixed(1)} ms`);
        fire("pointerup", 100 + 119 * 5, 100 + 119 * 3);
      }
    };
    requestAnimationFrame(step);
  }

  function clear() {
    dragStart.current = null;
    resizing.current = null;
    busy.current = false;
    pending.current = null;
    setMouse(null);
    setSel(null);
    setChosen(null);
    setLoaded(false);
    const c = canvasRef.current;
    if (c) c.width = c.height = 1; // release the frame's memory
  }

  function cancel() {
    const my = token.current;
    if (my) void api.overlayCancel(my).catch((e) => log(`overlay cancel failed: ${e}`));
  }

  function setInteraction(active: boolean) {
    const my = token.current;
    if (my) void invoke<boolean>("overlay_interaction", { token: my, active }).catch((e) => log(`overlay interaction failed: ${e}`));
  }

  function cancelDrag() {
    pending.current = null;
    resizing.current = null;
    if (!dragStart.current) return;
    dragStart.current = null;
    setSel(null);
    setInteraction(false);
  }

  const norm = (s: Sel) => ({ x: Math.min(s.x0, s.x1), y: Math.min(s.y0, s.y1), w: Math.abs(s.x1 - s.x0), h: Math.abs(s.y1 - s.y0) });

  function run(s: Sel, action?: ActionId) {
    const { x, y, w, h } = norm(s);
    // a selection resized down to (almost) nothing keeps the overlay: the backend would drop the capture
    if (busy.current || w < 4 || h < 4) return;
    busy.current = true;
    const my = token.current;
    api
      .overlayDone(my, x, y, w, h, action)
      .then(() => { if (token.current === my) clear(); })
      .catch((e) => {
        // The backend keeps the overlay only while the capture can still be used (a selection clipped to nothing)
        // and cancels it otherwise (overlay-cancelled then clears this page). Clearing here regardless, as 0.5.6
        // did, left a blank full-screen window on top of everything when the backend kept it.
        log(`overlay done failed: ${e}`);
        if (token.current !== my) return;
        busy.current = false;
        dragStart.current = null;
        setSel(null);
        setChosen(null);
        setInteraction(false);
      });
  }

  function finish(s: Sel) {
    const { w, h } = norm(s);
    if (w < 4 || h < 4 || busy.current) {
      setSel(null);
      setInteraction(false);
      return;
    }
    if (mode !== "select") return run(s); // direct hotkeys: the mode is the action
    if (opts.current.autoTranslate) return run(s, "translate");
    setChosen(s); // show the action toolbar
  }

  const shown = chosen ?? sel;
  const rect = shown && {
    left: Math.min(shown.x0, shown.x1),
    top: Math.min(shown.y0, shown.y1),
    width: Math.abs(shown.x1 - shown.x0),
    height: Math.abs(shown.y1 - shown.y0),
  };
  // place by the bar's measured size so it never runs off the right or bottom edge
  useLayoutEffect(() => {
    const el = barRef.current;
    if (!el) return;
    const w = Math.ceil(el.offsetWidth);
    const h = Math.ceil(el.offsetHeight);
    if (!barSize || barSize.w !== w || barSize.h !== h) setBarSize({ w, h });
  });
  const tb = chosen && rect ? toolbarPosition(rect, window.innerWidth, window.innerHeight, barSize) : null;
  const def = opts.current.defaultAction;

  return (
    <div
      className="overlay"
      onContextMenu={(e) => {
        e.preventDefault();
        cancel();
      }}
      onPointerDown={(e) => {
        if (!loaded || busy.current || e.button !== 0 || !e.isPrimary) return;
        if (e.isTrusted) e.currentTarget.setPointerCapture(e.pointerId);
        setInteraction(true);
        // a new drag replaces a finished selection
        setChosen(null);
        const [x, y] = localPoint(e);
        dragStart.current = [x, y];
        setSel({ x0: x, y0: y, x1: x, y1: y });
      }}
      onPointerMove={(e) => { if (e.isPrimary) onMoveCoalesced(...localPoint(e)); }}
      onPointerUp={(e) => {
        if (e.button !== 0 || !e.isPrimary) return;
        pending.current = null;
        const edges = resizing.current;
        resizing.current = null;
        const [x, y] = localPoint(e);
        if (edges) {
          setChosen((c) => c && resizeSelection(c, edges, x, y));
          return;
        }
        const anchor = dragStart.current;
        dragStart.current = null;
        if (anchor) finish({ x0: anchor[0], y0: anchor[1], x1: x, y1: y });
      }}
      onPointerCancel={cancelDrag}
      onLostPointerCapture={cancelDrag}
    >
      <canvas ref={canvasRef} className="overlay-img" style={{ visibility: loaded ? "visible" : "hidden" }} />
      {loaded &&
        (rect ? (
          <div className="overlay-sel" style={rect}>
            <span className="overlay-size">
              {Math.round(rect.width)} × {Math.round(rect.height)}
            </span>
            <span className="overlay-frame" />
            {selectionHandles(rect.width, rect.height).map(([fx, fy]) => (
              <span
                key={`${fx}${fy}`}
                className={`handle ${chosen ? "live" : ""}`}
                style={{ left: `${fx * 100}%`, top: `${fy * 100}%`, cursor: chosen ? HANDLE_CURSOR[`${fx}${fy}`] : undefined }}
                onPointerDown={(e) => {
                  if (!chosen || busy.current || e.button !== 0 || !e.isPrimary) return;
                  e.stopPropagation();
                  if (e.isTrusted) e.currentTarget.setPointerCapture(e.pointerId);
                  resizing.current = { l: fx === 0, r: fx === 1, t: fy === 0, b: fy === 1 };
                }}
              />
            ))}
          </div>
        ) : (
          <div className="overlay-dim" />
        ))}
      {loaded && !rect && mouse && (
        <>
          <div className="overlay-cross-h" style={{ top: mouse[1] }} />
          <div className="overlay-cross-v" style={{ left: mouse[0] }} />
        </>
      )}
      {loaded && chosen && tb && (
        <div ref={barRef} className="action-bar" style={tb} onPointerDown={(e) => e.stopPropagation()} onPointerUp={(e) => e.stopPropagation()}>
          {ACTIONS.map((a) => (
            <button
              key={a.id}
              className={`action ${a.id === def ? "default" : ""}`}
              title={`${a.label}（${a.num ? `${a.num} 或 ` : ""}${a.key}${a.id === def ? "，回车" : ""}）`}
              onClick={() => run(chosen, a.id)}
            >
              {a.num && <span className="num-badge">{a.num}</span>}
              <ActionIcon id={a.id} />
              <span>{a.label}</span>
            </button>
          ))}
          <span className="action-sep" />
          <button className="action icon-only" title="取消（Esc）" onClick={cancel}>
            <ActionIcon id="cancel" />
          </button>
        </div>
      )}
      {loaded && !dragStart.current && !chosen && (
        <div className="overlay-hint">
          <span className="hint-mode">{MODE_LABEL[mode]}</span>
          <span>拖动选择区域</span>
          <span className="hint-keys">Esc 取消</span>
          <button className="overlay-cancel" onPointerDown={(e) => e.stopPropagation()} onClick={cancel}>
            取消
          </button>
        </div>
      )}
    </div>
  );
}
