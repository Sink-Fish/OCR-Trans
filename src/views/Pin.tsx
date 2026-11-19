import { WorkingStatus } from "../lib/WorkingStatus";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { currentMonitor, monitorFromPoint, PhysicalPosition, PhysicalSize, LogicalPosition, LogicalSize, primaryMonitor } from "@tauri-apps/api/window";
import { win } from "../lib/win";
import { api, isWindows, loadBitmap, type PinInfo, type PipelineResult } from "../lib/api";
import { pinAnchor, pinLayout, type Box } from "../lib/placement";
import { paragraphAt, renderTranslated } from "../lib/render";
import { Icon, type IconId } from "../lib/icons";
import { mosaicCovers, bounds, drawAnnotations, drawSelection, drawShape, finishShape, nextMarkerNumber, shapeAt, translate as moveShape, type Shape, type Tool } from "../lib/doodle";

const COLORS = ["#ff3b30", "#ffcc00", "#34c759", "#0a84ff", "#000000"];
const MOD = navigator.userAgent.includes("Mac") ? "⌘" : "Ctrl+";
const WIDTHS = [2, 4, 8];

const SHAPES: { id: Tool; icon: IconId; title: string }[] = [
  { id: "rect", icon: "rect", title: "矩形" },
  { id: "ellipse", icon: "ellipse", title: "椭圆" },
  { id: "arrow", icon: "arrow", title: "箭头" },
];
const SHAPE_IDS = SHAPES.map((t) => t.id);
/** Annotation row (second toolbar row, only in 标注 mode); rect/ellipse/arrow share one button. */
const DRAW_TOOLS: { id: Tool; icon: IconId; title: string }[] = [
  { id: "select", icon: "select", title: "选择标注：拖动移动，Delete 删除" },
  { id: "pen", icon: "pen", title: "画笔" },
  { id: "highlight", icon: "highlight", title: "荧光笔" },
  { id: "text", icon: "text", title: "文字" },
  { id: "marker", icon: "marker", title: "序号（自动递增）" },
  { id: "mosaic", icon: "mosaic", title: "马赛克" },
];
const TOOLS: { id: Tool; icon: IconId; title: string }[] = [
  { id: "move", icon: "move", title: "移动钉图（拖动）" },
  { id: "select", icon: "select", title: "选择标注：拖动移动，Delete 删除" },
  { id: "pen", icon: "pen", title: "画笔" },
  { id: "highlight", icon: "highlight", title: "荧光笔" },
  { id: "rect", icon: "rect", title: "矩形" },
  { id: "ellipse", icon: "ellipse", title: "椭圆" },
  { id: "arrow", icon: "arrow", title: "箭头" },
  { id: "text", icon: "text", title: "文字" },
  { id: "marker", icon: "marker", title: "序号（自动递增）" },
  { id: "mosaic", icon: "mosaic", title: "马赛克" },
];

/** One shape of every kind (dev: OCRTRANS_TEST_DOODLES) to check rendering without a mouse. */
function demoShapes(w: number, h: number, k: number): Shape[] {
  return [
    { kind: "pen", color: "#ff3b30", width: 4 * k, points: Array.from({ length: 20 }, (_, i) => [w * 0.05 + i * 6 * k, h * 0.2 + Math.sin(i / 2) * 10 * k] as [number, number]) },
    { kind: "highlight", color: "#ffcc00", width: 4 * k, points: [[w * 0.05, h * 0.45], [w * 0.45, h * 0.45]] },
    { kind: "rect", color: "#34c759", width: 3 * k, x0: w * 0.5, y0: h * 0.1, x1: w * 0.7, y1: h * 0.4 },
    { kind: "ellipse", color: "#0a84ff", width: 3 * k, x0: w * 0.72, y0: h * 0.1, x1: w * 0.95, y1: h * 0.4 },
    { kind: "arrow", color: "#ff3b30", width: 3 * k, x0: w * 0.55, y0: h * 0.8, x1: w * 0.75, y1: h * 0.55 },
    { kind: "text", color: "#000000", size: 18 * k, x: w * 0.05, y: h * 0.6, text: "标注文字 Note" },
    { kind: "marker", color: "#ff3b30", size: 12 * k, x: w * 0.85, y: h * 0.7, n: 1 },
    { kind: "marker", color: "#0a84ff", size: 12 * k, x: w * 0.92, y: h * 0.7, n: 2 },
    { kind: "mosaic", block: 8 * k, x0: w * 0.3, y0: h * 0.75, x1: w * 0.5, y1: h * 0.95 },
  ];
}

export default function Pin({ id, translate, demo = false, initialTool }: { id: number; translate: boolean; demo?: boolean; initialTool?: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [info, setInfo] = useState<PinInfo | null>(null);
  const [img, setImg] = useState<ImageBitmap | null>(null);
  const [result, setResult] = useState<PipelineResult | null>(null);
  const [clean, setClean] = useState<ImageBitmap | null>(null);
  const [tip, setTip] = useState<{ x: number; y: number; index: number } | null>(null);
  const [paraEdit, setParaEdit] = useState<{ index: number; left: number; top: number; width: number; height: number; value: string } | null>(null);
  const [showTranslated, setShowTranslated] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const zoomed = useRef(false);
  const [opacity, setOpacity] = useState(1);
  const [tool, setTool] = useState<Tool>(TOOLS.some((t) => t.id === initialTool) ? (initialTool as Tool) : "move");
  const [color, setColor] = useState(COLORS[0]);
  const [width, setWidth] = useState(WIDTHS[1]);
  // annotation history: shapes = current, past/future for undo/redo
  const [shapes, setShapesRaw] = useState<Shape[]>([]);
  const [past, setPast] = useState<Shape[][]>([]);
  const [future, setFuture] = useState<Shape[][]>([]);
  const [draft, setDraft] = useState<Shape | null>(null);
  const [selected, setSelected] = useState(-1);
  // drag state for the select tool (original shape + start point) and deferred window drag
  const dragRef = useRef<{ index: number; orig: Shape; x: number; y: number; moved: boolean } | null>(null);
  const pressRef = useRef<{ x: number; y: number } | null>(null);
  const pointerRef = useRef<number | null>(null);
  const [textEdit, setTextEdit] = useState<{ x: number; y: number; cssX: number; cssY: number; value: string } | null>(null);
  const [hover, setHover] = useState(true);
  // toolbar: annotation row on/off, which inline sub-row is open, last used shape
  const [annotate, setAnnotate] = useState(!!initialTool && initialTool !== "move");
  const [sub, setSub] = useState<null | "shapes" | "colors" | "more">(null);
  const [shapeTool, setShapeTool] = useState<Tool>("rect");
  // window layout (image + toolbar offsets inside the window), see lib/placement.ts
  const [lay, setLay] = useState<{ img: { x: number; y: number }; tb: { x: number; y: number } } | null>(null);
  const toolbarRef = useRef<HTMLDivElement>(null);
  const imgOffset = useRef({ x: 0, y: 0 });
  const layoutQueue = useRef(Promise.resolve());
  const shown = useRef(false);

  const commit = useCallback(
    (next: Shape[]) => {
      setPast((p) => [...p.slice(-99), shapes]);
      setFuture([]);
      setShapesRaw(next);
    },
    [shapes],
  );
  const undo = useCallback(() => {
    if (!past.length) return;
    setFuture((f) => [shapes, ...f]);
    setShapesRaw(past[past.length - 1]);
    setPast((p) => p.slice(0, -1));
    setSelected(-1);
  }, [past, shapes]);
  const redo = useCallback(() => {
    if (!future.length) return;
    setPast((p) => [...p, shapes]);
    setShapesRaw(future[0]);
    setFuture((f) => f.slice(1));
    setSelected(-1);
  }, [future, shapes]);

  const flash = useCallback((msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast((t) => (t === msg ? null : t)), 2200);
  }, []);

  // load image + info
  useEffect(() => {
    let alive = true;
    (async () => {
      const i = await api.pinInfo(id);
      if (!alive) return;
      const bitmap = await loadBitmap(`pin/${id}`);
      if (!alive) {
        bitmap.close();
        return;
      }
      setInfo(i);
      setImg(bitmap);
      if (demo) {
        setShapesRaw(demoShapes(i.width, i.height, i.scale));
        setTool("select");
        setAnnotate(true);
        setSelected(3);
      }
    })().catch((e) => {
      if (alive) {
        void api.toast(`钉图加载失败：${e}`, "error");
        void api.closeSelf();
      }
    });
    return () => { alive = false; };
  }, [id, demo]);

  // Each decoded screenshot owns native image memory until explicitly closed.
  useEffect(() => () => img?.close(), [img]);
  useEffect(() => () => clean?.close(), [clean]);

  const font = info?.font_family ?? "sans-serif";
  const scale = info?.scale ?? 1;
  const cssW = info ? (info.width / scale) * zoom : 0;
  const cssH = info ? (info.height / scale) * zoom : 0;

  // Lay the window out around the image: the image stays where it was captured, the toolbar goes below / above /
  // inside it and is clamped to the monitor's visible work area (menu bar and Dock excluded).
  const isSelect = tool === "select";
  useLayoutEffect(() => {
    const tbEl = toolbarRef.current;
    if (!info || !img || !tbEl) return;
    const tb = { w: Math.ceil(tbEl.offsetWidth), h: Math.ceil(tbEl.offsetHeight) };
    let cancelled = false;
    // The native resize can move the window before its position is restored. A subsequent layout
    // must read the position after both operations complete, along with the matching image offset.
    layoutQueue.current = layoutQueue.current.then(async () => {
      if (cancelled) return;
      const anchor = isWindows && !shown.current ? info.physical_anchor : undefined;
      const current = anchor ? await monitorFromPoint(...anchor) : await currentMonitor();
      const mon = current ?? await primaryMonitor();
      const sf = isWindows && mon ? mon.scaleFactor : await win.scaleFactor();
      const pos = anchor ? { x: anchor[0] / sf, y: anchor[1] / sf } : (await win.outerPosition()).toLogical(sf);
      const imgScreen = { x: pos.x + imgOffset.current.x, y: pos.y + imgOffset.current.y, w: Math.ceil(cssW), h: Math.ceil(cssH) };
      const work: Box = mon
        ? {
            x: mon.workArea.position.x / mon.scaleFactor,
            y: mon.workArea.position.y / mon.scaleFactor,
            w: mon.workArea.size.width / mon.scaleFactor,
            h: mon.workArea.size.height / mon.scaleFactor,
          }
        : { x: 0, y: 0, w: window.screen.availWidth, h: window.screen.availHeight };
      const display = current ? {
        x: current.position.x / current.scaleFactor,
        y: current.position.y / current.scaleFactor,
        w: current.size.width / current.scaleFactor,
        h: current.size.height / current.scaleFactor,
      } : null;
      // A saved capture can reopen after its original monitor has been disconnected.
      const L = pinLayout(shown.current ? imgScreen : pinAnchor(imgScreen, display, work), tb, work);
      if (cancelled) return;
      // size first: AppKit keeps the bottom-left corner fixed when resizing
      if (isWindows) {
        await win.setPosition(new PhysicalPosition(Math.round(L.window.x * sf), Math.round(L.window.y * sf)));
        await win.setSize(new PhysicalSize(Math.ceil(L.window.w * sf), Math.ceil(L.window.h * sf)));
      } else {
        await win.setSize(new LogicalSize(Math.ceil(L.window.w), Math.ceil(L.window.h)));
        await win.setPosition(new LogicalPosition(L.window.x, L.window.y));
      }
      imgOffset.current = L.image;
      setLay({ img: L.image, tb: L.toolbar });
      if (!shown.current && !cancelled) {
        await win.show();
        shown.current = true;
        await win.setFocus();
      }
    }).catch((e) => {
      if (cancelled) return;
      if (shown.current) flash(`钉图布局失败：${e}`);
      else {
        void api.toast(`钉图打开失败：${e}`, "error");
        void api.closeSelf();
      }
    });
    return () => {
      cancelled = true;
    };
  }, [cssW, cssH, info, img, annotate, sub, isSelect, result?.engine, showTranslated, flash]);

  const translatedCanvas = useMemo(
    () => (clean && result && info ? renderTranslated(clean, result, info.font_family) : null),
    [clean, result, info],
  );

  // only the latest translation request may update the pin (快/精 clicked again before the first one finished)
  const request = useRef(0);
  useEffect(() => () => { request.current++; }, [id]);
  const runTranslate = useCallback(async (engine?: "fast" | "quality") => {
    if (!img || !info) return;
    const my = ++request.current;
    setBusy(engine === "quality" ? "高质量翻译中…" : "翻译中…");
    try {
      const r = await api.pinProcess(id, true, engine);
      if (my !== request.current) return;
      const cleanImg = await loadBitmap(`pin/${id}/clean`);
      if (my !== request.current) {
        cleanImg.close();
        return;
      }
      setClean(cleanImg);
      setResult(r);
      const n = r.paragraphs.filter((p) => p.translation).length;
      setShowTranslated(n > 0);
      if (r.warnings.length) flash(r.warnings.join("；"));
      else if (n === 0) flash(r.paragraphs.length ? "没有需要翻译的英文/日文" : "未识别到文字");
    } catch (e) {
      if (my === request.current) flash(`翻译失败：${e}`);
    } finally {
      if (my === request.current) setBusy(null);
    }
  }, [id, img, info, flash]);

  useEffect(() => {
    if (img && translate) runTranslate();
  }, [img, translate, runTranslate]);

  const base = showTranslated && translatedCanvas ? translatedCanvas : img;
  // The preview adds the draft and selection around the committed composition used by exports.
  useEffect(() => {
    const c = canvasRef.current;
    if (!c || !base) return;
    const ctx = drawAnnotations(c, base, shapes, font);
    if (draft) drawShape(ctx, draft, c, font);
    if (tool === "select" && shapes[selected]) drawSelection(ctx, bounds(shapes[selected], font), scale / zoom);
  }, [base, shapes, draft, font, selected, tool, scale, zoom]);

  useEffect(() => {
    if (!showTranslated || !translatedCanvas || !result || !new URLSearchParams(location.search).has("verify")) return;
    const frame = requestAnimationFrame(() => {
      if (canvasRef.current?.width === translatedCanvas.width) {
        void api.releaseProbe(result.engine, result.warnings.length).catch(console.warn);
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [showTranslated, translatedCanvas, result]);

  const toImg = (e: { clientX: number; clientY: number }): [number, number] => {
    const r = canvasRef.current!.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * info!.width;
    const y = Math.max(0, Math.min(1, (e.clientY - r.top) / r.height)) * info!.height;
    return [x, y];
  };

  function commitText() {
    if (textEdit && textEdit.value.trim()) {
      commit([...shapes, { kind: "text", color, size: Math.round(18 * scale * (width / 4 + 0.5)), x: textEdit.x, y: textEdit.y, text: textEdit.value }]);
    }
    setTextEdit(null);
  }

  function onPointerDown(e: React.PointerEvent<HTMLCanvasElement>) {
    if (!e.isPrimary || e.button !== 0 || !info || pointerRef.current !== null) return;
    if (textEdit) {
      commitText();
      return;
    }
    if (tool !== "move" && tool !== "marker" && tool !== "text") {
      e.currentTarget.setPointerCapture(e.pointerId);
      pointerRef.current = e.pointerId;
    }
    const [x, y] = toImg(e);
    const w = width * scale;
    switch (tool) {
      case "move":
        // start the OS window drag only once the mouse actually moves, so clicks/double-clicks still work
        pressRef.current = { x: e.clientX, y: e.clientY };
        return;
      case "select": {
        const i = shapeAt(shapes, x, y, 6 * scale / zoom, font);
        setSelected(i);
        dragRef.current = i >= 0 ? { index: i, orig: shapes[i], x, y, moved: false } : null;
        return;
      }
      case "pen":
      case "highlight":
        setDraft({ kind: tool, color, width: w, points: [[x, y]] });
        break;
      case "rect":
      case "ellipse":
      case "arrow":
        setDraft({ kind: tool, color, width: w, x0: x, y0: y, x1: x, y1: y });
        break;
      case "marker":
        commit([...shapes, { kind: "marker", color, size: Math.round(11 * scale * (width / 4 + 0.6)), x, y, n: nextMarkerNumber(shapes) }]);
        break;
      case "mosaic":
        setDraft({ kind: "mosaic", block: Math.round(6 * scale * (width / 4 + 0.5)), x0: x, y0: y, x1: x, y1: y });
        break;
      case "text": {
        const r = canvasRef.current!.getBoundingClientRect();
        setTextEdit({ x, y, cssX: e.clientX - r.left, cssY: e.clientY - r.top, value: "" });
        break;
      }
    }
  }

  const isLlm = result?.engine.startsWith("llm") ?? false;

  /** CSS rect of a paragraph's bbox inside the stage. */
  function cssRect(bbox: [number, number, number, number]) {
    const k = cssW / info!.width;
    return { left: bbox[0] * k, top: bbox[1] * k, width: bbox[2] * k, height: bbox[3] * k };
  }

  function hitParagraph(e: React.MouseEvent): number {
    if (!showTranslated || !result || !info) return -1;
    const [x, y] = toImg(e);
    const i = paragraphAt(result, x, y);
    return i >= 0 && result.paragraphs[i].translation ? i : -1;
  }

  function onDoubleClick(e: React.MouseEvent) {
    if (tool !== "move" || busy) return;
    const i = hitParagraph(e);
    if (i < 0) {
      api.closeSelf();
      return;
    }
    const p = result!.paragraphs[i];
    if (mosaicCovers(p.bbox, shapes)) return;
    setTip(null);
    setParaEdit({ index: i, ...cssRect(p.bbox), value: p.translation ?? "" });
  }

  async function commitParaEdit() {
    if (!paraEdit || !result) return;
    const { index, value } = paraEdit;
    setParaEdit(null);
    if (value === result.paragraphs[index].translation) return;
    try {
      await api.pinSetTranslation(id, index, value);
      // A retranslation may have completed while the edit was being saved.
      setResult((current) => current !== result ? current : {
        ...current, paragraphs: current.paragraphs.map((p, i) => (i === index ? { ...p, translation: value } : p)),
      });
    } catch (err) {
      setParaEdit(current => current ?? paraEdit);
      flash(String(err));
    }
  }

  function onPointerMove(e: React.PointerEvent<HTMLCanvasElement>) {
    if (!e.isPrimary) return;
    if (pressRef.current && e.buttons & 1) {
      if (Math.hypot(e.clientX - pressRef.current.x, e.clientY - pressRef.current.y) > 3) {
        pressRef.current = null;
        setTip(null);
        win.startDragging().catch((e) => flash(`拖动失败：${e}`));
      }
      return;
    }
    if (dragRef.current && e.buttons & 1) {
      const d = dragRef.current;
      const [x, y] = toImg(e);
      d.moved = true;
      const moved = moveShape(d.orig, x - d.x, y - d.y);
      setShapesRaw((s) => s.map((sh, i) => (i === d.index ? moved : sh)));
      return;
    }
    if (tool === "move" && !draft) {
      const i = hitParagraph(e);
      if (i >= 0 && !paraEdit && !mosaicCovers(result!.paragraphs[i].bbox, shapes)) {
        const r = canvasRef.current!.getBoundingClientRect();
        setTip({ x: e.clientX - r.left, y: e.clientY - r.top, index: i });
      } else if (tip) setTip(null);
    }
    if (!draft || pointerRef.current !== e.pointerId) return;
    const [x, y] = toImg(e);
    setDraft((d) => {
      if (!d) return d;
      if (d.kind === "pen" || d.kind === "highlight") return { ...d, points: [...d.points, [x, y]] };
      if (d.kind === "text" || d.kind === "marker") return d;
      return { ...d, x1: x, y1: y };
    });
  }

  function onPointerUp(e: React.PointerEvent<HTMLCanvasElement>) {
    pressRef.current = null;
    if (pointerRef.current !== e.pointerId) return;
    pointerRef.current = null;
    const [x, y] = toImg(e);
    const d = dragRef.current;
    if (d) {
      dragRef.current = null;
      if (d.moved || x !== d.x || y !== d.y) {
        // record the move as one undoable step: history gets the pre-drag state
        setPast((p) => [...p.slice(-99), shapes.map((sh, i) => (i === d.index ? d.orig : sh))]);
        setFuture([]);
        setShapesRaw((s) => s.map((shape, i) => i === d.index ? moveShape(d.orig, x - d.x, y - d.y) : shape));
      }
    }
    if (!draft) return;
    commit([...shapes, finishShape(draft, x, y)]);
    setDraft(null);
  }

  function cancelPointer(pointerId: number) {
    if (pointerRef.current !== pointerId) return;
    pointerRef.current = null;
    const canvas = canvasRef.current;
    if (canvas?.hasPointerCapture(pointerId)) canvas.releasePointerCapture(pointerId);
    const d = dragRef.current;
    dragRef.current = null;
    if (d?.moved) setShapesRaw((s) => s.map((shape, i) => i === d.index ? d.orig : shape));
    setDraft(null);
  }

  function deleteSelected() {
    if (selected < 0 || !shapes[selected]) return;
    commit(shapes.filter((_, i) => i !== selected));
    setSelected(-1);
  }

  function onWheel(e: React.WheelEvent) {
    if (e.deltaY === 0) return;
    // ⌥/⌘ (Alt on Windows) + wheel = opacity. Ctrl + wheel is what a trackpad pinch sends, so it zooms.
    if (e.altKey || e.metaKey) {
      setOpacity((o) => Math.min(1, Math.max(0.15, o + (e.deltaY < 0 ? 0.08 : -0.08))));
    } else {
      zoomed.current = true;
      const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
      setZoom((z) => Math.min(5, Math.max(0.2, z * factor)));
    }
  }

  useEffect(() => {
    if (zoomed.current) flash(`${Math.round(zoom * 100)}%`);
  }, [zoom, flash]);

  async function pngBytes(): Promise<Uint8Array> {
    if (!base) throw new Error("截图尚未加载");
    if (pointerRef.current !== null) throw new Error("请先完成当前标注");
    const canvas = document.createElement("canvas");
    drawAnnotations(canvas, base, shapes, font);
    const blob: Blob = await new Promise((res, rej) => canvas.toBlob((b) => (b ? res(b) : rej("图片编码失败")), "image/png"));
    return new Uint8Array(await blob.arrayBuffer());
  }

  /** 识字: the shared text window (select / edit / copy the text), on the translation if that is showing. */
  function openText() {
    api.openTextWindow(id, showTranslated).catch((e) => flash(String(e)));
  }

  async function copyImage() {
    try {
      await api.copyPng(await pngBytes());
      flash("图片已复制");
    } catch (e) {
      flash(String(e));
    }
  }

  async function saveImage() {
    try {
      const p = await api.savePng(await pngBytes());
      if (p) flash("已保存");
    } catch (e) {
      flash(String(e));
    }
  }

  const hasTranslation = translatedCanvas && result?.paragraphs.some((p) => p.translation !== null);

  function toggleTranslate() {
    if (hasTranslation) setShowTranslated((v) => !v);
    else runTranslate();
  }

  // the backend asks an already open pin to show its translation (钉图 from the text window's 译文 view)
  const showTranslation = useRef(() => {});
  showTranslation.current = () => {
    if (hasTranslation) setShowTranslated(true);
    else runTranslate();
  };
  useEffect(() => {
    const un = win.listen("pin-show-translation", () => showTranslation.current());
    // the backend's progress while a translation runs ("正在加载高质量翻译模型…" → "高质量翻译中…")
    const un2 = win.listen<string>("pin-busy", (e) => setBusy((b) => (b ? e.payload : b)));
    return () => {
      un.then((f) => f());
      un2.then((f) => f());
    };
  }, []);

  function switchEngine() {
    runTranslate(isLlm ? "fast" : "quality");
  }

  // keyboard
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (textEdit || paraEdit || e.isComposing || e.keyCode === 229) return;
      const target = e.target;
      if (target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return;
      const mod = e.metaKey || e.ctrlKey;
      if (e.key === "Escape") {
        e.preventDefault();
        if (pointerRef.current !== null) cancelPointer(pointerRef.current);
        else if (selected >= 0) setSelected(-1);
        else if (tool !== "move") setTool("move");
        else api.closeSelf();
      } else if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (pointerRef.current !== null) return cancelPointer(pointerRef.current);
        if (e.shiftKey) redo();
        else undo();
      } else if (mod && e.key.toLowerCase() === "y") {
        e.preventDefault();
        if (pointerRef.current !== null) return;
        redo();
      } else if ((e.key === "Delete" || e.key === "Backspace") && tool === "select") {
        e.preventDefault();
        if (pointerRef.current !== null) return;
        deleteSelected();
      } else if (mod && e.key.toLowerCase() === "c") {
        e.preventDefault();
        copyImage();
      } else if (mod && e.key.toLowerCase() === "s") {
        e.preventDefault();
        saveImage();
      } else if (!mod && (e.key === " " || e.key.toLowerCase() === "t")) {
        e.preventDefault();
        if (e.repeat || busy) return;
        toggleTranslate();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const pick = (t: Tool) => {
    setTool(t);
    setSelected(-1);
  };
  const colorDot = <i style={{ background: COLORS.includes(color) ? color : color }} />;

  return (
    <div className="pin" onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)} style={{ opacity }}>
      <div className="pin-stage" style={{ width: cssW, height: cssH, left: lay?.img.x ?? 0, top: lay?.img.y ?? 0 }} onWheel={onWheel}>
        <canvas
          ref={canvasRef}
          className={`pin-canvas tool-${tool}`}
          style={{ width: cssW, height: cssH }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={(e) => cancelPointer(e.pointerId)}
          onLostPointerCapture={(e) => cancelPointer(e.pointerId)}
          onPointerLeave={() => {
            pressRef.current = null;
            setTip(null);
          }}
          onDoubleClick={onDoubleClick}
        />
        <div className="pin-frame" />
        {tip && tool === "move" && showTranslated && result?.paragraphs[tip.index] && !mosaicCovers(result.paragraphs[tip.index].bbox, shapes) && (
          <div className="pin-tip" style={{ left: Math.min(tip.x + 12, Math.max(0, cssW - 260)), top: tip.y + 16 }}>
            {result.paragraphs[tip.index].text}
            <span className="muted">（双击修改译文）</span>
          </div>
        )}
        {paraEdit && (
          <textarea
            autoFocus
            className="pin-para-edit"
            style={{ left: paraEdit.left, top: paraEdit.top, width: Math.max(paraEdit.width, 160), minHeight: paraEdit.height }}
            value={paraEdit.value}
            onChange={(e) => setParaEdit({ ...paraEdit, value: e.target.value })}
            onKeyDown={(e) => {
              // Enter that confirms an input-method composition (pinyin → English letters) is not a submit
              if (e.nativeEvent.isComposing || e.keyCode === 229) return;
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                commitParaEdit();
              } else if (e.key === "Escape") setParaEdit(null);
            }}
            onBlur={commitParaEdit}
          />
        )}
        {textEdit && (
          <textarea
            autoFocus
            className="pin-text-input"
            style={{ left: textEdit.cssX, top: textEdit.cssY, color, fontSize: Math.round(18 * (width / 4 + 0.5) * zoom) }}
            value={textEdit.value}
            onChange={(e) => setTextEdit({ ...textEdit, value: e.target.value })}
            onKeyDown={(e) => {
              // Enter that confirms an input-method composition (pinyin → English letters) is not a submit
              if (e.nativeEvent.isComposing || e.keyCode === 229) return;
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                commitText();
              } else if (e.key === "Escape") setTextEdit(null);
            }}
            onBlur={commitText}
          />
        )}
        {busy && <div className="pin-shimmer" />}
        {busy && <div className="pin-busy"><WorkingStatus label={busy} /><button className="sm" style={{ pointerEvents: "auto" }} onClick={() => api.pinCancel(id).catch(e => flash(String(e)))}>取消</button></div>}
        {toast && (
          <div className="pin-toast" key={toast}>
            {!toast.endsWith("%") && <Icon id="check" />}
            {toast}
          </div>
        )}
        {showTranslated && <div className="pin-badge">译</div>}
      </div>

      <div ref={toolbarRef} className={`pin-toolbar ${hover || annotate || sub ? "" : "hidden"}`} style={{ left: lay?.tb.x ?? 0, top: lay?.tb.y ?? 0, visibility: lay ? "visible" : "hidden" }}>
        {/* row 1: the main actions */}
        <div className="tb-row">
          <button title="移动钉图（拖动）" className={tool === "move" ? "active" : ""} onClick={() => pick("move")}>
            <Icon id="move" />
          </button>
          <button
            title="标注工具"
            className={`wide ${annotate ? "active" : ""}`}
            onClick={() => {
              const on = !annotate;
              setAnnotate(on);
              setSub(null);
              pick(on ? "pen" : "move");
            }}
          >
            <Icon id="pen" />
            标注
          </button>
          <span className="sep" />
          <button title="原文 / 译文 切换 (T)" className={`wide ${showTranslated ? "active" : ""}`} onClick={toggleTranslate}>
            <Icon id="translate" />
            {showTranslated ? "原文" : "翻译"}
          </button>
          <button title="识字：选择、编辑、复制文字" className="wide" onClick={openText}>
            <Icon id="ocr" />
            识字
          </button>
          <button title={`复制图片 (${MOD}C)`} onClick={copyImage}>
            <Icon id="image" />
          </button>
          <button title={`保存 (${MOD}S)`} onClick={saveImage}>
            <Icon id="save" />
          </button>
          <button title="更多" className={sub === "more" ? "active" : ""} onClick={() => setSub(sub === "more" ? null : "more")}>
            <span className="dots">⋯</span>
          </button>
          <button title="关闭 (Esc / 双击)" onClick={() => api.closeSelf()}>
            <Icon id="close" />
          </button>
        </div>

        {/* row 2: annotation tools, only in 标注 mode */}
        {annotate && (
          <div className="tb-row">
            {DRAW_TOOLS.slice(0, 3).map((t) => (
              <button key={t.id} title={t.title} className={tool === t.id ? "active" : ""} onClick={() => pick(t.id)}>
                <Icon id={t.icon} />
              </button>
            ))}
            <button
              title="形状：矩形 / 椭圆 / 箭头"
              className={`caret ${SHAPE_IDS.includes(tool) ? "active" : ""}`}
              onClick={() => {
                pick(shapeTool);
                setSub(sub === "shapes" ? null : "shapes");
              }}
            >
              <Icon id={SHAPES.find((x) => x.id === shapeTool)!.icon} />
              <span className="chev">▾</span>
            </button>
            {DRAW_TOOLS.slice(3).map((t) => (
              <button key={t.id} title={t.title} className={tool === t.id ? "active" : ""} onClick={() => pick(t.id)}>
                <Icon id={t.icon} />
              </button>
            ))}
            <span className="sep" />
            <button title="颜色与粗细" className={`caret swatch ${sub === "colors" ? "active" : ""}`} onClick={() => setSub(sub === "colors" ? null : "colors")}>
              {colorDot}
              <span className="chev">▾</span>
            </button>
            <span className="sep" />
            <button title={`撤销 (${MOD}Z)`} disabled={!past.length} onClick={undo}>
              <Icon id="undo" />
            </button>
            <button title={`重做 (${MOD === "⌘" ? "⌘⇧Z" : "Ctrl+Shift+Z"})`} disabled={!future.length} onClick={redo}>
              <Icon id="redo" />
            </button>
            {tool === "select" && (
              <button title="删除选中标注 (Delete)" disabled={selected < 0} onClick={deleteSelected}>
                <Icon id="delete" />
              </button>
            )}
          </div>
        )}

        {/* inline sub-rows ("popovers" that stay inside the window) */}
        {sub === "shapes" && (
          <div className="tb-row sub">
            {SHAPES.map((t) => (
              <button
                key={t.id}
                title={t.title}
                className={`wide ${tool === t.id ? "active" : ""}`}
                onClick={() => {
                  setShapeTool(t.id);
                  pick(t.id);
                  setSub(null);
                }}
              >
                <Icon id={t.icon} />
                {t.title}
              </button>
            ))}
          </div>
        )}
        {sub === "colors" && (
          <div className="tb-row sub">
            {COLORS.map((c) => (
              <button key={c} title="颜色" className={`swatch ${color === c ? "active" : ""}`} onClick={() => setColor(c)}>
                <i style={{ background: c }} />
              </button>
            ))}
            <label title="自定义颜色" className={`swatch picker ${COLORS.includes(color) ? "" : "active"}`}>
              <i style={{ background: COLORS.includes(color) ? "conic-gradient(red, yellow, lime, cyan, blue, magenta, red)" : color }} />
              <input type="color" value={color} onChange={(e) => setColor(e.target.value)} />
            </label>
            <span className="sep" />
            {WIDTHS.map((w) => (
              <button key={w} title={`粗细 ${w}`} className={width === w ? "active" : ""} onClick={() => setWidth(w)}>
                <i className="width-dot" style={{ width: w + 3, height: w + 3 }} />
              </button>
            ))}
          </div>
        )}
        {sub === "more" && (
          <div className="tb-row sub">
            <span className="tb-label">透明度</span>
            {[1, 0.8, 0.6, 0.4].map((o) => (
              <button key={o} className={`wide ${Math.abs(opacity - o) < 0.01 ? "active" : ""}`} onClick={() => setOpacity(o)}>
                {Math.round(o * 100)}%
              </button>
            ))}
            {(result?.engine || info?.has_saved_translation) && (
              <>
                <span className="sep" />
                {(result?.engine === "history" || (!result && info?.has_saved_translation)) && <button className="wide" onClick={() => runTranslate("fast")}>快速重译</button>}
                <button title={info?.has_saved_translation && !isLlm ? "用高质量模型重新翻译" : isLlm ? "当前：高质量翻译（Qwen3）。点击改用快速翻译" : "当前：快速翻译。点击用高质量大模型重新翻译"} className="wide" onClick={switchEngine}>
                  {isLlm ? "改用快速翻译" : "高质量重译"}
                </button>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
