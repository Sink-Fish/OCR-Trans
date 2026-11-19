import { WorkingStatus } from "../lib/WorkingStatus";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { win } from "../lib/win";
import { api, plainText, shotUrl, type PinInfo, type PipelineResult } from "../lib/api";
import { buildTextMap, dragRange, imageSelectionText, lineRangeAt, rangePolys, shouldAutoCopy, type Pt } from "../lib/textmap";
import { Icon } from "../lib/icons";

const isMac = navigator.userAgent.includes("Mac");

/**
 * The one text window (识字 / the pin's 识字 button): the capture with a selectable text layer on top (drag across
 * words or lines like Live Text) and an editable copy of all the text, kept in sync with the selection.
 * Nothing is copied unless the user asks (or turned on "识字后自动复制全部").
 *
 * Showing the window runs once per pin. OCR reruns only after an explicit retry.
 */
export default function TextWindow({ id, translated, autotest }: { id: number; translated?: boolean; autotest?: string }) {
  const [info, setInfo] = useState<PinInfo | null>(null);
  const [result, setResult] = useState<PipelineResult | null>(null);
  const [src, setSrc] = useState("");
  const [tr, setTr] = useState<string | null>(null);
  const [mode, setMode] = useState<"src" | "tr">("src");
  const [sel, setSel] = useState<[number, number]>([0, 0]);
  const [status, setStatus] = useState("识别中…");
  const [ocrFailed, setOcrFailed] = useState(false);
  const [ocrAttempt, setOcrAttempt] = useState(0);
  const [translationRetry, setTranslationRetry] = useState(false);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [fit, setFit] = useState<{ w: number; h: number } | null>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);
  const dragFrom = useRef<Pt | null>(null);

  const map = useMemo(() => (result ? buildTextMap(result) : null), [result]);
  const text = mode === "tr" && tr !== null ? tr : src;
  // the image ↔ editor link holds while the editor shows the untouched original text
  const synced = mode === "src" && map !== null && src === map.text;

  const hasEdits = useRef(false);
  const translating = useRef(false);
  useEffect(() => {
    let active = true;
    const unlisten = win.listen<string>("pin-busy", event => { if (active && translating.current) setStatus(event.payload); });
    return () => { active = false; void unlisten.then(f => f()).catch(console.warn); };
  }, [id]);

  function flash(msg: string) {
    setToast(msg);
    window.setTimeout(() => setToast((t) => (t === msg ? null : t)), 1800);
  }

  function acceptTranslation(r: PipelineResult): boolean {
    const translated = r.paragraphs.some((p) => p.translation?.trim());
    const t = plainText(r, true);
    if (translated) setTr(t);
    setTranslationRetry(r.warnings.length > 0);
    setStatus(r.warnings.length ? r.warnings.join("；") : translated ? `译文 ${t.length} 字` : "没有需要翻译的英文/日文");
    return translated;
  }

  async function loadTranslation(): Promise<boolean> {
    if (translating.current) return false;
    translating.current = true;
    setBusy(true);
    setStatus("翻译中…");
    try {
      return acceptTranslation(await api.pinProcess(id, true));
    } catch (e) {
      setTranslationRetry(true);
      setStatus(`翻译失败：${e}`);
      return false;
    } finally {
      translating.current = false;
      setBusy(false);
    }
  }

  useEffect(() => {
    let alive = true;
    void win.show().then(
      () => {
        if (alive) void win.setFocus().catch((e) => console.warn("聚焦识字窗口失败", e));
      },
      (e) => {
        if (alive) void api.toast(`显示识字窗口失败：${e}`, "error").catch(console.warn);
      },
    );
    return () => {
      alive = false;
    };
  }, [id]);

  useEffect(() => {
    let alive = true;
    (async () => {
      const [i, s] = await Promise.all([api.pinInfo(id), api.getSettings().catch(() => null)]);
      if (!alive) return;
      setInfo(i);
      const r = await api.pinProcess(id, false);
      if (!alive) return;
      setResult(r);
      const t = plainText(r, false);
      setSrc(t);
      setStatus(t.trim() ? `${t.length} 字 · ${Math.round(r.timings.total_ms)} ms` : "未识别到文字");
      if (autotest === "hold") {
        // dev hook (screenshots): stay open with the second line selected on the image
        const m = buildTextMap(r);
        const l = m.lines[Math.min(1, m.lines.length - 1)];
        if (l) setSel([l.start, l.end]);
        return;
      }
      if (autotest) {
        // dev hook: translate (layout changes), then close right away
        await loadTranslation();
        setMode("tr");
        window.setTimeout(() => (autotest === "js" ? win.close() : api.closeSelf()), 60);
        return;
      }
      if (shouldAutoCopy(s?.ocr_auto_copy, t)) {
        void api.copyText(t).then(
          () => { if (alive) flash(`已自动复制全部 ${t.length} 字`); },
          (e) => { if (alive) flash(`自动复制失败：${e}`); },
        );
      }
      if (translated && t.trim()) {
        // OCR returns the pin's cached result, including any translation already shown on it.
        const hasTranslation = r.paragraphs.some((p) => p.translation !== null) || r.warnings.length > 0;
        if ((hasTranslation ? acceptTranslation(r) : await loadTranslation()) && alive) setMode("tr");
      }
    })().catch((e) => {
      if (alive) {
        setOcrFailed(true);
        setStatus(`识别失败：${e}`);
      }
    });
    return () => {
      alive = false;
    };
  }, [id, ocrAttempt]);

  // fit the image into its pane (never upscaled past its captured size)
  useLayoutEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      const w = Math.floor(e.contentRect.width);
      const h = Math.floor(e.contentRect.height);
      setFit((f) => (f && f.w === w && f.h === h ? f : { w, h }));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const lw = info ? info.width / info.scale : 0;
  const lh = info ? info.height / info.scale : 0;
  const k = info && fit ? Math.min(1, fit.w / lw, fit.h / lh) : 0;

  function toImg(e: React.PointerEvent | React.MouseEvent): Pt {
    const r = svgRef.current!.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * info!.width, ((e.clientY - r.top) / r.height) * info!.height];
  }

  /** Put [a, b) into the editor's selection (and show it there) when the two are linked. */
  function selectInEditor(a: number, b: number, focus: boolean) {
    const ta = taRef.current;
    if (!ta || !synced) return;
    if (focus) ta.focus({ preventScroll: true });
    ta.setSelectionRange(a, b);
  }

  function startDrag(e: React.PointerEvent<SVGSVGElement>) {
    if (!map || e.button !== 0) return;
    if (src !== map.text) return flash("原文已编辑，请先还原再从图片选字");
    e.preventDefault();
    if (mode === "tr") setMode("src");
    taRef.current?.blur();
    const p = toImg(e);
    dragFrom.current = p;
    e.currentTarget.setPointerCapture(e.pointerId);
    const r = dragRange(map, p, p);
    if (r) setSel(r);
  }

  function moveDrag(e: React.PointerEvent<SVGSVGElement>) {
    if (!map || !dragFrom.current) return;
    const r = dragRange(map, dragFrom.current, toImg(e));
    if (r && (r[0] !== sel[0] || r[1] !== sel[1])) setSel(r);
  }

  function endDrag() {
    if (!dragFrom.current) return;
    dragFrom.current = null;
    if (sel[1] > sel[0]) selectInEditor(sel[0], sel[1], true);
  }

  function selectLine(e: React.MouseEvent<SVGSVGElement>) {
    if (!map || src !== map.text) return;
    const r = lineRangeAt(map, ...toImg(e));
    if (!r) return;
    setSel(r);
    selectInEditor(r[0], r[1], true);
  }

  /** What ⌘C copies: the editor's selection, else the selection made on the image. Empty = nothing selected. */
  function selectedText(): string {
    const ta = taRef.current;
    if (ta && document.activeElement === ta && ta.selectionEnd > ta.selectionStart) return ta.value.slice(ta.selectionStart, ta.selectionEnd);
    // the image selection indexes the original text: in the 译文 view only the editor's own selection counts
    if (mode === "src" && map) return imageSelectionText(map, src, sel);
    return "";
  }

  async function copy(t: string, what: string) {
    if (!t.trim()) return flash("没有可复制的文字");
    try {
      await api.copyText(t);
      flash(`已复制${what} ${t.length} 字`);
    } catch (e) {
      flash(`复制失败：${e}`);
    }
  }

  async function copyAllAndClose() {
    if (!text.trim()) return flash("没有可复制的文字");
    try {
      await api.copyText(text);
    } catch (e) {
      return flash(`复制失败：${e}`);
    }
    void api.toast(`已复制 ${text.length} 字`).catch(console.warn);
    await api.closeSelf().catch((e) => flash(`关闭失败：${e}`));
  }

  async function showTranslation() {
    if (tr !== null || (await loadTranslation())) setMode("tr");
  }

  function retryOcr() {
    setOcrFailed(false);
    setStatus("识别中…");
    setOcrAttempt((n) => n + 1);
  }

  // keyboard: one listener for the window's lifetime, always calling the latest handler
  const onKey = useRef<(e: KeyboardEvent) => void>(() => {});
  onKey.current = (e: KeyboardEvent) => {
    if (e.isComposing || e.keyCode === 229) return;
    const mod = isMac ? e.metaKey : e.ctrlKey;
    const inEditor = document.activeElement === taRef.current;
    if (e.key === "Escape") {
      e.preventDefault();
      api.closeSelf();
    } else if (mod && e.key === "Enter") {
      e.preventDefault();
      copyAllAndClose();
    } else if (mod && e.key.toLowerCase() === "c") {
      e.preventDefault();
      const t = selectedText();
      if (t) copy(t, "选中的");
      else copy(text, "全部");
    } else if (mod && e.key.toLowerCase() === "a" && !inEditor && map) {
      e.preventDefault();
      setSel([0, map.text.length]);
    }
  };
  useEffect(() => {
    const f = (e: KeyboardEvent) => onKey.current(e);
    window.addEventListener("keydown", f);
    return () => window.removeEventListener("keydown", f);
  }, []);

  const polys = synced && map ? rangePolys(map, sel[0], sel[1]) : [];
  const stacked = lw / Math.max(lh, 1) >= 1.4;
  const nSel = sel[1] - sel[0];

  return (
    <div className={`tw ${stacked ? "stacked" : "side"}`}>
      <div className="tw-stage" ref={stageRef}>
        {info && k > 0 && (
          <div className="tw-img" style={{ width: lw * k, height: lh * k }}>
            <img src={shotUrl(`pin/${id}`)} alt="" draggable={false} />
            <svg
              ref={svgRef}
              viewBox={`0 0 ${info.width} ${info.height}`}
              preserveAspectRatio="none"
              className={map && src === map.text && map.lines.length ? "has-text" : ""}
              onPointerDown={startDrag}
              onPointerMove={moveDrag}
              onPointerUp={endDrag}
              onPointerCancel={endDrag}
              onDoubleClick={selectLine}
            >
              {map?.lines.map((l, i) => (
                <polygon key={`l${i}`} className="tw-line" points={l.quad.map((p) => p.join(",")).join(" ")} />
              ))}
              {polys.map((p, i) => (
                <polygon key={`s${i}`} className="tw-sel" points={p.map((q) => q.join(",")).join(" ")} />
              ))}
            </svg>
          </div>
        )}
        {!result && !ocrFailed && <div className="tw-shimmer" />}
      </div>

      <div className="tw-side">
        <div className="tw-head">
          <div className="seg small">
            <button className={mode === "src" ? "on" : ""} onClick={() => setMode("src")}>
              原文
            </button>
            <button className={mode === "tr" ? "on" : ""} disabled={!result?.paragraphs.length || busy} onClick={showTranslation}>
              译文
            </button>
          </div>
          <span className="tw-status" title={status}>
            {busy ? <WorkingStatus label={status} /> : synced && nSel > 0 ? `已选 ${nSel} 字 · ${isMac ? "⌘C" : "Ctrl+C"} 复制` : status}
            {mode === "src" && map && !synced && (
              <>
                {" · 已编辑 "}
                <button className="link" onClick={() => setSrc(map.text)}>
                  还原
                </button>
              </>
            )}
          </span>
          {busy && <button className="link" onClick={() => api.pinCancel(id).catch(e => flash(String(e)))}>取消翻译</button>}
          {ocrFailed && <button className="link" onClick={retryOcr}>重试识字</button>}
          {translationRetry && !busy && (
            <button className="link" onClick={async () => { if (await loadTranslation()) setMode("tr"); }}>重试翻译</button>
          )}
        </div>
        <textarea
          ref={taRef}
          className="tw-text"
          value={text}
          placeholder={result ? "未识别到文字" : status}
          spellCheck={false}
          onChange={(e) => { hasEdits.current = true; (mode === "tr" ? setTr : setSrc)(e.target.value); }}
          onSelect={(e) => {
            if (!synced) return;
            const t = e.currentTarget;
            if (t.selectionStart !== sel[0] || t.selectionEnd !== sel[1]) setSel([t.selectionStart, t.selectionEnd]);
          }}
        />
        <div className="tw-bar">
          <button
            title="钉到屏幕上"
            onClick={async () => {
              try {
                await api.openPin(id, mode === "tr");
                if (!hasEdits.current) api.closeSelf();
              } catch (e) {
                flash(`钉图失败：${e}`);
              }
            }}
          >
            <Icon id="pin" size={14} />
            钉图
          </button>
          <span className="tw-flex" />
          <button title={`复制全部并关闭（${isMac ? "⌘↩" : "Ctrl+Enter"}）`} onClick={copyAllAndClose} disabled={!text.trim()}>
            复制并关闭
          </button>
          <button className="primary" title="复制全部文字" onClick={() => copy(text, "全部")} disabled={!text.trim()}>
            <Icon id="copy" size={14} />
            一键复制全部
          </button>
        </div>
      </div>
      {toast && <div className="tw-toast">{toast}</div>}
    </div>
  );
}
