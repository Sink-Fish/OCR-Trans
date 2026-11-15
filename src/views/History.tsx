import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, shotUrl, type HistoryItem } from "../lib/api";
import { Logo } from "../lib/Logo";
import { EmptyArt } from "../lib/icons";

function when(sec: number): string {
  const d = new Date(sec * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getMonth() + 1}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

const MODE: Record<string, string> = { pin: "钉图", ocr: "识字", translate: "翻译" };

/** Last 50 captures: thumbnail, time, text; re-pin, copy, delete, clear. */
export default function History() {
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [msg, setMsg] = useState<{ text: string; error: boolean } | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [loadError, setLoadError] = useState("");
  const revision = useRef(0);

  const flash = useCallback((text: string, error = false) => {
    const message = { text, error };
    setMsg(message);
    window.setTimeout(() => setMsg((current) => current === message ? null : current), 3000);
  }, []);

  async function run(action: Promise<unknown>, success?: string) {
    try {
      await action;
      if (success) flash(success);
    } catch (err) {
      flash(`操作失败：${err}`, true);
    }
  }

  useEffect(() => {
    let alive = true;
    async function refresh() {
      const current = ++revision.current;
      try {
        const entries = await api.historyList();
        if (alive && current === revision.current) { setItems(entries); setLoadError(""); }
      } catch (err) {
        if (alive && current === revision.current) setLoadError(String(err));
      }
    }
    const un = listen("history-changed", refresh);
    const initial = () => { if (alive) void refresh(); };
    void un.then(initial, initial);
    return () => {
      alive = false;
      void un.then((f) => f()).catch(() => {});
    };
  }, [flash]);

  return (
    <div className="history">
      <header>
        <span className="brand">
          <Logo size={20} />
        </span>
        <h3>截图历史</h3>
        <span className="muted small">最近 {items.length} / 50 条，保存在本机应用数据目录</span>
        <span style={{ flex: 1 }} />
        {msg && <span className={`tag ${msg.error ? "warn" : "ok"}`} role="status">{msg.text}</span>}
        {confirmClear ? (
          <>
            <span className="small">确定清空全部历史？</span>
            <button
              className="danger"
              onClick={() => run(api.historyClear().then(() => setConfirmClear(false)))}
            >
              清空
            </button>
            <button onClick={() => setConfirmClear(false)}>取消</button>
          </>
        ) : (
          <button className="sm ghost" disabled={!items.length} onClick={() => setConfirmClear(true)}>
            清空历史
          </button>
        )}
      </header>
      {loadError ? (
        <div className="empty-state" role="alert"><b>无法读取历史记录</b><span>{loadError}</span><button onClick={() => run(api.historyRecover())}>恢复历史索引</button></div>
      ) : items.length === 0 ? (
        <div className="empty-state">
          <EmptyArt />
          <b>还没有截图</b>
          <span>按 {navigator.userAgent.includes("Mac") ? "⌥⇧O" : "Alt+Shift+O"} 框选屏幕，最近 50 次截图会出现在这里。</span>
          <button className="primary" style={{ marginTop: 8 }} onClick={() => run(api.startCapture("select"))}>
            开始截图
          </button>
        </div>
      ) : (
        <div className="history-grid">
          {items.map((it) => (
            <div key={it.id} className="history-item">
              <div className="thumb" onDoubleClick={() => run(api.historyPin(it.id, false))} title="双击重新钉图">
                <img src={shotUrl(`history/${it.id}`)} alt="" loading="lazy" />
              </div>
              <div className="meta">
                <span className="tag">{MODE[it.mode] ?? it.mode}</span>
                <span className="muted small">{when(it.created)}</span>
                <span className="muted small">
                  {Math.round(it.width / it.scale)}×{Math.round(it.height / it.scale)}
                </span>
              </div>
              <div className="snippet small" title={it.translation || it.text}>
                {(it.translation || it.text || "（未识别）").slice(0, 80)}
              </div>
              <div className="row">
                <button className="sm" onClick={() => run(api.historyPin(it.id, false))}>
                  钉图
                </button>
                <button className="sm primary" onClick={() => run(api.historyPin(it.id, true))}>
                  翻译
                </button>
                <button
                  className="sm"
                  disabled={!it.text}
                  onClick={() => run(api.copyText(it.translation || it.text), "已复制")}
                >
                  复制
                </button>
                <button className="sm ghost" title="删除" onClick={() => run(api.historyDelete(it.id))}>
                  删除
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
